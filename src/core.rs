use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Instant;

use colored::Colorize;
use tree_sitter::Tree;

use crate::cache::{load_cache, save_cache, MutationId};
use crate::config::Config;
use crate::emit;
use crate::io::FileGuard;
use crate::mutation::MutationPoint;
use crate::parser;
use crate::runner::{self, TestRun};

// ── Interrupt signalling ────────────────────────────────

/// 0 = no interrupt, 1 = soft stop (finish current, then stop), 2 = hard (abort now)
static INTERRUPT_STATE: AtomicU8 = AtomicU8::new(0);

/// Set once at startup. The handler is installed in run_mutation_testing.
static HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Install the Ctrl-C handler. Idempotent — safe to call multiple times.
fn install_ctrlc_handler() {
    if HANDLER_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = ctrlc::set_handler(move || {
        let old = INTERRUPT_STATE.fetch_add(1, Ordering::SeqCst);
        if old == 0 {
            eprintln!("\n⏸  Interrupted — finishing current mutation, then stopping...");
            eprintln!("   (press Ctrl-C again to quit immediately)");
        } else {
            // second Ctrl-C: immediate exit
            eprintln!("\n💥 Second interrupt — quitting now.");
            std::process::exit(130);
        }
    });
}

fn interrupted() -> bool {
    INTERRUPT_STATE.load(Ordering::SeqCst) >= 1
}

/// Recursively dump CST nodes with byte ranges and node IDs.
/// Each node is indented; leaf nodes get a `*` marker.
fn dump_cst_node(node: &tree_sitter::Node, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let text = &source[node.start_byte()..node.end_byte()];
    let short = if text.len() > 70 {
        format!("{}…", &text[..70].replace('\n', "\\n").replace('\t', "\\t"))
    } else {
        text.replace('\n', "\\n").replace('\t', "\\t")
    };
    let leaf = if node.child_count() == 0 { " *" } else { "" };
    println!(
        "{}[{:4}]  kind=\"{}\"  bytes={}..{}  \"{}\"{}",
        indent,
        node.id(),
        node.kind(),
        node.start_byte(),
        node.end_byte(),
        short,
        leaf,
    );
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            dump_cst_node(&child, source, depth + 1);
        }
    }
}

// ── Data types ──────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum MutationOutcome {
    Killed,
    Survived,
    Error,
    Skipped,
}

#[derive(Debug)]
pub struct MutationResult {
    pub point: MutationPoint,
    pub outcome: MutationOutcome,
}

impl MutationResult {
    pub fn killed(&self) -> bool {
        matches!(self.outcome, MutationOutcome::Killed)
    }
    pub fn survived(&self) -> bool {
        matches!(self.outcome, MutationOutcome::Survived)
    }
    pub fn errored(&self) -> bool {
        matches!(self.outcome, MutationOutcome::Error)
    }
    pub fn skipped(&self) -> bool {
        matches!(self.outcome, MutationOutcome::Skipped)
    }
}

struct FileTree {
    source: String,
    tree: Tree,
}

// ── Main entry point ────────────────────────────────────

/// When `path` is a file, walk up to find the project root (directory containing
/// Gemfile, .git, or spec/). Returns the original path unchanged if it's already
/// a directory or no project root is found.
///
/// Relative paths are resolved against the current working directory so
/// parent-walk works correctly from any directory. Symlinks are NOT followed
/// (unlike canonicalize) so prefix matching against WalkDir output stays consistent.
fn find_project_root(path: &str) -> String {
    let p = std::path::Path::new(path);

    // Resolve relative paths against CWD (no symlink following).
    let resolved = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(p)
    };

    if resolved.is_dir() {
        return resolved.to_string_lossy().to_string();
    }

    let mut current = resolved.parent();
    while let Some(dir) = current {
        if dir.join("Gemfile").exists()
            || dir.join("spec").is_dir()
            || dir.join(".git").exists()
        {
            return dir.to_string_lossy().to_string();
        }
        current = dir.parent();
    }
    // Fallback: use the parent directory of the resolved path, or the original.
    resolved
        .parent()
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Run mutation testing on a Ruby project using the given config.
pub fn run_mutation_testing(cfg: &Config) -> anyhow::Result<Vec<MutationResult>> {
    install_ctrlc_handler();
    INTERRUPT_STATE.store(0, Ordering::SeqCst);

    let project_path = cfg.project_path;

    // When the user passes a single file (e.g. `-p app/models/user.rb`),
    // we still need the project root for cwd and spec derivation.
    // find_project_root walks up from the file to find Gemfile/spec/.git.
    let project_root = find_project_root(project_path);

    // Step 1: Find Ruby source files
    let rb_files: Vec<String> = walkdir::WalkDir::new(project_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rb"))
        .filter(|e| {
            let p = e.path().to_string_lossy();
            !p.contains("/spec/")
                && !p.contains("/test/")
                && !p.contains("/vendor/")
                && !p.ends_with("_spec.rb")
        })
        .map(|e| e.path().to_string_lossy().to_string())
        .collect();

    if rb_files.is_empty() {
        anyhow::bail!("No .rb source files found in {}", project_path);
    }

    // Step 2: Parse once, collect mutations. Only keep trees for files
    // that have mutation points (avoid holding CSTs for 593 files in memory).
    let mut file_trees: std::collections::HashMap<String, FileTree> =
        std::collections::HashMap::new();
    let mut all_points: Vec<MutationPoint> = Vec::new();

    for file in &rb_files {
        let source = std::fs::read_to_string(file)?;
        let tree = parser::parse_source(&source)?;

        if cfg.dump_cst {
            println!("\n══════ {} ══════\n", file);
            dump_cst_node(&tree.root_node(), &source, 0);
        }

        let points = parser::find_mutations(&tree, &source, file);
        if !points.is_empty() {
            file_trees.insert(file.clone(), FileTree { source, tree });
        }
        all_points.extend(points);
    }

    if cfg.dump_cst {
        return Ok(vec![]);
    }

    println!(
        "Found {} mutation points across {} Ruby files",
        all_points.len(),
        rb_files.len()
    );

    if all_points.is_empty() {
        println!("Nothing to mutate. Exiting.");
        return Ok(vec![]);
    }

    if let Some(n) = cfg.limit {
        if n < all_points.len() {
            all_points.truncate(n);
            println!("Limited to first {} mutation(s)\n", n);
        }
    }

    // Step 2.5: List-only mode — print and exit before mutating anything
    if cfg.list_only {
        for point in &all_points {
            let source_line = file_trees
                .get(&point.file)
                .and_then(|ft| ft.source.lines().nth(point.line_number.saturating_sub(1)))
                .unwrap_or("");
            println!(
                "{}:{}  {} -> {}  [{}]",
                point.file, point.line_number, point.original,
                point.replacement, point.operator_name,
            );
            println!("  {}", source_line);
        }
        println!("\n{} mutation point(s) found.", all_points.len());
        return Ok(vec![]);
    }

    // Step 2.6: Cache
    let mut results = Vec::new();
    let cache: Option<(std::collections::HashSet<MutationId>, std::path::PathBuf)> =
        cfg.cache_path.map(|p| {
            let path = std::path::PathBuf::from(p);
            let killed_set = load_cache(&path);
            (killed_set, path)
        });

    if let Some((ref killed_set, _)) = cache {
        let (to_run, skipped): (Vec<_>, Vec<_>) = all_points
            .into_iter()
            .partition(|p| !killed_set.contains(&MutationId::from_point(p)));

        for pt in skipped {
            if cfg.verbosity.show_detail() {
                println!(
                    "SKIP {}:{}  {} -> {}  [cached as killed]",
                    pt.file, pt.line_number, pt.original, pt.replacement,
                );
            }
            results.push(MutationResult {
                point: pt,
                outcome: MutationOutcome::Skipped,
            });
        }
        all_points = to_run;
    }

    if all_points.is_empty() {
        println!(
            "All {} mutations were previously killed. Nothing to test.",
            results.len()
        );
        return Ok(results);
    }

    println!(
        "{} new mutation(s) to test, {} skipped from cache\n",
        all_points.len(),
        results.len()
    );

    // Step 3: Baseline, unless user is using {spec_file} template — in that case
    // the baseline would run a different test suite than each mutation (full vs.
    // targeted), so it's neither comparable nor useful.
    let uses_targeted_spec = cfg.test_cmd.map_or(false, |cmd| cmd.contains("{spec_file}"));

    if uses_targeted_spec {
        println!("Skipping baseline (targeted spec mode — each mutation runs its own spec)\n");
    } else {
        println!("Running baseline test suite...");
        let baseline_start = Instant::now();
        let baseline = runner::run_tests(&project_root, cfg.test_cmd, None)?;
        let baseline_duration = baseline_start.elapsed();

        if cfg.verbosity.show_always() {
            println!("--- baseline output ---");
            print!("{}", baseline.stdout);
            if !baseline.stderr.is_empty() {
                eprint!("{}", baseline.stderr);
            }
            println!("--- end baseline ---\n");
        }

        if baseline.outcome == runner::TestOutcome::Error {
            eprintln!(
                "WARNING: Baseline test suite could not run — all mutations will report as errors.\n"
            );
        } else {
            let total_est = baseline_duration * all_points.len() as u32;
            println!(
                "Baseline: {:?} per run ~ est. total: ~{:?} for {} mutations\n",
                baseline_duration,
                total_est,
                all_points.len()
            );
        }
    }

    // Step 4: Test each mutation
    let total = all_points.len();
    let start_time = Instant::now();
    let mut newly_killed: Vec<MutationId> = Vec::new();

    let warn_label = "WARNING:".yellow().bold();
    let crit_label = "CRITICAL:".red().bold();

    for (i, point) in all_points.iter().enumerate() {
        let ft = &file_trees[&point.file];
        let mutated = emit::emit_tree(&ft.tree, &ft.source, point.node_id, &point.replacement);

        let mut guard = FileGuard::overwrite(std::path::Path::new(&point.file), &mutated)?;

        let spec_file = runner::derive_spec_file(&point.file, &project_root);
        let spec_file_str = spec_file.as_deref();
        let test_run = runner::run_tests(&project_root, cfg.test_cmd, spec_file_str)
            .unwrap_or_else(|_| TestRun {
                outcome: runner::TestOutcome::Error,
                stdout: String::new(),
                stderr: "run_tests failed".into(),
                exit_code: None,
            });

        // Restore the original file and verify it actually happened.
        // In Rails apps with Spring/Bootsnap, the test process may hold a
        // cached copy of the mutated file and write it back after we rename.
        if let Err(e) = guard.restore() {
            eprintln!(
                "{} Failed to restore {} after mutation: {}. \
                 File may be left in mutated state.",
                warn_label,
                point.file.dimmed(),
                e,
            );
        } else {
            // Verify: read back the file and check it matches the original source.
            // If it doesn't, Spring or a test hook has written back the mutated
            // version after our restore — force-write the original content.
            match std::fs::read_to_string(&point.file) {
                Ok(current) if current == ft.source => {
                    // Restore confirmed — file matches original.
                }
                Ok(current) => {
                    let lines_match = current.lines().count() == ft.source.lines().count();
                    eprintln!(
                        "{} {} was overwritten after restore ({} lines, {} → {} len). \
                         Re-writing original content.",
                        warn_label,
                        point.file.dimmed(),
                        if lines_match { "same" } else { "different" },
                        ft.source.len(),
                        current.len(),
                    );
                    if let Err(e) = std::fs::write(&point.file, &ft.source) {
                        eprintln!(
                            "{} Could not repair {} — file is left in mutated state: {}",
                            crit_label,
                            point.file.dimmed(),
                            e,
                        );
                    } else {
                        eprintln!("  Repaired successfully.");
                    }
                }
                Err(e) => {
                    eprintln!(
                        "{} Could not verify restore of {}: {}. \
                         File may be left in mutated state.",
                        warn_label,
                        point.file.dimmed(),
                        e,
                    );
                }
            }
        }

        let mutation_outcome = match test_run.outcome {
            runner::TestOutcome::Pass => MutationOutcome::Survived,
            runner::TestOutcome::Fail => {
                newly_killed.push(MutationId::from_point(point));
                MutationOutcome::Killed
            }
            runner::TestOutcome::Error => MutationOutcome::Error,
        };

        let elapsed = start_time.elapsed();
        let done = i + 1;
        let remaining = total - done;
        let avg_per = elapsed / done as u32;
        let eta = avg_per * remaining as u32;

        let (outcome_str, outcome_color) = match mutation_outcome {
            MutationOutcome::Killed => ("KILLED", "green"),
            MutationOutcome::Survived => ("SURVIVED", "red"),
            MutationOutcome::Error => ("ERROR", "yellow"),
            MutationOutcome::Skipped => ("SKIPPED", "dimmed"),
        };

        // Colored progress line: framing dimmed, mutation signal white, outcome bracket bold + colored
        println!(
            "[{}/{}] {}:{}  {} -> {}  {}  est. remaining: ~{:?}",
            done.to_string().dimmed(),
            total.to_string().dimmed(),
            point.file.dimmed(),
            point.line_number.to_string().dimmed(),
            point.original,
            point.replacement,
            format!("[{} / {}]", outcome_str, point.operator_name).color(outcome_color).bold(),
            format!("{:?}", eta).dimmed(),
        );

        let show = mutation_outcome == MutationOutcome::Survived
            || mutation_outcome == MutationOutcome::Error;
        if (show && cfg.verbosity.show_detail()) || cfg.verbosity.show_always() {
            // Test output left uncolored — creates visual contrast with rubigo framing
            if !test_run.stdout.is_empty() {
                println!("{}", test_run.stdout);
            }
            if !test_run.stderr.is_empty() {
                eprintln!("{}", test_run.stderr);
            }
        }

        results.push(MutationResult {
            point: point.clone(),
            outcome: mutation_outcome,
        });

        // Check interrupt AFTER file is restored
        if interrupted() {
            println!("\n🛑 Stopped after {}/{} mutations (user interrupted)", done, total);
            break;
        }
    }

    // Save cache once at the end (batched)
    if let Some((_, ref cache_path)) = cache {
        if !newly_killed.is_empty() {
            if let Err(e) = save_cache(cache_path, &newly_killed) {
                eprintln!("Warning: could not save cache: {}", e);
            }
        }
    }

    let error_count = results.iter().filter(|r| r.errored()).count();
    if error_count > 0 {
        eprintln!(
            "WARNING: {} mutation(s) could not be tested due to test suite errors.",
            error_count
        );
    }

    Ok(results)
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Verbosity;
    use std::sync::atomic::Ordering;

    fn make_point(
        file: &str,
        line: usize,
        node_id: usize,
        original: &str,
        replacement: &str,
    ) -> MutationPoint {
        MutationPoint {
            file: file.to_string(),
            line_number: line,
            node_id,
            original: original.to_string(),
            replacement: replacement.to_string(),
            operator_name: "flip_equality".to_string(),
        }
    }

    #[test]
    fn test_killed() {
        let r = MutationResult {
            point: make_point("a.rb", 1, 0, "==", "!="),
            outcome: MutationOutcome::Killed,
        };
        assert!(r.killed());
        assert!(!r.survived() && !r.errored() && !r.skipped());
    }

    #[test]
    fn test_survived() {
        let r = MutationResult {
            point: make_point("a.rb", 1, 0, "==", "!="),
            outcome: MutationOutcome::Survived,
        };
        assert!(r.survived());
        assert!(!r.killed() && !r.errored() && !r.skipped());
    }

    #[test]
    fn test_errored() {
        let r = MutationResult {
            point: make_point("a.rb", 1, 0, "==", "!="),
            outcome: MutationOutcome::Error,
        };
        assert!(r.errored());
        assert!(!r.killed() && !r.survived() && !r.skipped());
    }

    #[test]
    fn test_skipped() {
        let r = MutationResult {
            point: make_point("a.rb", 1, 0, "==", "!="),
            outcome: MutationOutcome::Skipped,
        };
        assert!(r.skipped());
        assert!(!r.killed() && !r.survived() && !r.errored());
    }

    #[test]
    fn test_interrupt_state_defaults_to_zero() {
        // Reset from any prior test that set it
        INTERRUPT_STATE.store(0, Ordering::SeqCst);
        assert!(!interrupted());
        assert_eq!(INTERRUPT_STATE.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_interrupt_detected_after_set() {
        INTERRUPT_STATE.store(0, Ordering::SeqCst);
        assert!(!interrupted());
        INTERRUPT_STATE.store(1, Ordering::SeqCst);
        assert!(interrupted());
        INTERRUPT_STATE.store(2, Ordering::SeqCst);
        assert!(interrupted());
        // Clean up
        INTERRUPT_STATE.store(0, Ordering::SeqCst);
    }

    // ── Integration-level tests ─────────────────────────

    #[test]
    fn test_rejects_non_ruby() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("echo ok && exit 0"),
            cache_path: None,
            limit: None,
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };
        let result = run_mutation_testing(&cfg);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No .rb source files"));
    }

    #[test]
    fn test_empty_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        std::fs::write(dir.path().join("lib").join("foo.rb"), "# nothing\n").unwrap();
        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("echo ok && exit 0"),
            cache_path: None,
            limit: None,
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };
        let _ = run_mutation_testing(&cfg);
    }

    #[test]
    fn test_limit_zero() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        std::fs::write(
            dir.path().join("lib").join("foo.rb"),
            "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n",
        )
        .unwrap();
        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("echo ok && exit 0"),
            cache_path: None,
            limit: Some(0),
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };
        let _ = run_mutation_testing(&cfg);
    }

    #[test]
    fn test_all_cached() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        let source = "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n";
        std::fs::write(dir.path().join("lib").join("foo.rb"), source).unwrap();

        let cache_path = dir.path().join("cache.json");
        crate::cache::save_cache(
            &cache_path,
            &[MutationId {
                file: dir
                    .path()
                    .join("lib/foo.rb")
                    .to_string_lossy()
                    .to_string(),
                line_number: 3,
                original: "==".to_string(),
            }],
        )
        .unwrap();

        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("echo ok && exit 0"),
            cache_path: Some(cache_path.to_str().unwrap()),
            limit: None,
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };
        let results = run_mutation_testing(&cfg).unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().all(|r| r.skipped()));
    }

    #[test]
    fn test_derive_spec_file_returns_none_when_no_spec() {
        // When a source file has no corresponding _spec.rb, derive_spec_file
        // returns None, and run_tests should fall back to full suite.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        // Create source but NO spec file
        std::fs::write(
            dir.path().join("lib").join("foo.rb"),
            "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n",
        )
        .unwrap();

        let project_path = dir.path().to_str().unwrap().to_string();
        let source_path = dir
            .path()
            .join("lib/foo.rb")
            .to_string_lossy()
            .to_string();

        let result = runner::derive_spec_file(&source_path, &project_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_derive_spec_file_returns_some_when_spec_exists() {
        // When the spec file exists, derive_spec_file returns it.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec/lib")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        // Create source AND spec file
        std::fs::write(
            dir.path().join("lib").join("foo.rb"),
            "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("spec/lib/foo_spec.rb"), "").unwrap();

        let project_path = dir.path().to_str().unwrap().to_string();
        let source_path = dir
            .path()
            .join("lib/foo.rb")
            .to_string_lossy()
            .to_string();

        let result = runner::derive_spec_file(&source_path, &project_path);
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("spec/lib/foo_spec.rb"));
    }

    #[test]
    fn test_targeted_spec_mutation_with_test_cmd() {
        // Run with a test-cmd using {spec_file} template against a project
        // that has both source and spec.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec/lib")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        std::fs::write(
            dir.path().join("lib").join("foo.rb"),
            "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("spec").join("lib").join("foo_spec.rb"),
            // spec always fails -> mutation appears killed
            "exit 1",
        )
        .unwrap();

        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("ruby {spec_file}"),
            cache_path: None,
            limit: Some(1),
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };

        let results = run_mutation_testing(&cfg).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].killed());
        assert_eq!(results[0].point.original, "==");
        assert_eq!(results[0].point.replacement, "!=");
    }

    #[test]
    fn test_spec_dir_rb_files_are_excluded() {
        // .rb files inside spec/ directory must be skipped entirely.
        // A file like spec/support/shared_examples.rb may have operator code
        // but should never be mutated.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec/support")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();

        // Real source in lib/
        std::fs::write(
            dir.path().join("lib").join("foo.rb"),
            "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n",
        )
        .unwrap();

        // Shared example in spec/support/ — has "==" but must be excluded
        std::fs::write(
            dir.path().join("spec/support/shared_examples.rb"),
            "RSpec.shared_examples 'comparable' do\n  it { expect(a == b).to be true }\nend\n",
        )
        .unwrap();

        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("echo ok && exit 0"),
            cache_path: None,
            limit: None,
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };

        let results = run_mutation_testing(&cfg).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].point.file.ends_with("lib/foo.rb"));
    }

    #[test]
    fn test_test_dir_rb_files_are_excluded() {
        // .rb files inside test/ directory (Minitest convention) excluded.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("test")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();

        // Source in lib/
        std::fs::write(
            dir.path().join("lib").join("foo.rb"),
            "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n",
        )
        .unwrap();

        // Test helper with operators — must be excluded
        std::fs::write(
            dir.path().join("test/test_helper.rb"),
            "class Minitest::Test\n  def assert_equal(a, b)\n    a != b\n  end\nend\n",
        )
        .unwrap();

        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("echo ok && exit 0"),
            cache_path: None,
            limit: None,
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };

        let results = run_mutation_testing(&cfg).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].point.file.ends_with("lib/foo.rb"));
    }

    #[test]
    fn test_vendor_dir_rb_files_are_excluded() {
        // .rb files inside vendor/ directory (bundled gems) excluded.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("vendor/bundle/gems/some_gem/lib")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();

        // Source in lib/
        std::fs::write(
            dir.path().join("lib").join("foo.rb"),
            "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n",
        )
        .unwrap();

        // Vendored gem — must be excluded
        std::fs::write(
            dir.path().join("vendor/bundle/gems/some_gem/lib/some_gem.rb"),
            "module SomeGem\n  def self.compare(a, b)\n    a == b\n  end\nend\n",
        )
        .unwrap();

        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("echo ok && exit 0"),
            cache_path: None,
            limit: None,
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };

        let results = run_mutation_testing(&cfg).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].point.file.ends_with("lib/foo.rb"));
    }

    #[test]
    fn test_spec_helper_excluded() {
        // spec/spec_helper.rb is inside spec/ directory — must be excluded.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();

        std::fs::write(
            dir.path().join("lib").join("foo.rb"),
            "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n",
        )
        .unwrap();

        // spec_helper with no operators — shouldn't be mutated anyway,
        // but the important thing is it's not even scanned.
        std::fs::write(
            dir.path().join("spec/spec_helper.rb"),
            "RSpec.configure { |c| c.mock_with :rspec }\n",
        )
        .unwrap();

        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("echo ok && exit 0"),
            cache_path: None,
            limit: None,
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };

        let results = run_mutation_testing(&cfg).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_only_source_files_in_spec_dir_errors() {
        // When ALL .rb files live in spec/ or test/ or vendor/,
        // we should get an error (no source files found).
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec/models")).unwrap();
        std::fs::write(
            dir.path().join("spec/models/user_spec.rb"),
            "RSpec.describe User do\n  it { expect(1 == 1).to be true }\nend\n",
        )
        .unwrap();

        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("echo ok && exit 0"),
            cache_path: None,
            limit: None,
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };

        let result = run_mutation_testing(&cfg);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No .rb source files"));
    }

    #[test]
    fn test_spec_rb_suffix_excluded_outside_spec_dir() {
        // _spec.rb files outside spec/ directory excluded by their suffix.
        // Catches the case where a spec file is co-located with source.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();

        std::fs::write(
            dir.path().join("lib").join("foo.rb"),
            "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n",
        )
        .unwrap();

        // _spec.rb accidentally in lib/ — has a mutation but must be excluded
        std::fs::write(
            dir.path().join("lib").join("foo_spec.rb"),
            "  a == b\n",
        )
        .unwrap();

        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("echo ok && exit 0"),
            cache_path: None,
            limit: None,
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };

        let results = run_mutation_testing(&cfg).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].point.file.ends_with("lib/foo.rb"));
    }

    #[test]
    fn test_non_rb_files_not_scanned() {
        // .erb, .rake, Gemfile, Rakefile, etc. should be ignored.
        // Only .rb files are collected.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        std::fs::write(dir.path().join("lib/foo.rb"), "# empty\n").unwrap();
        std::fs::write(dir.path().join("lib/template.erb"), "<%= 1 == 2 %>\n").unwrap();
        std::fs::write(dir.path().join("Rakefile"), "task :default do; end\n").unwrap();
        std::fs::write(dir.path().join("Gemfile"), "source 'https://rubygems.org'\n").unwrap();

        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: Some("echo ok && exit 0"),
            cache_path: None,
            limit: None,
            list_only: false,
            verbosity: Verbosity::Quiet,
            dump_cst: false,
        };

        let results = run_mutation_testing(&cfg).unwrap();
        // Only lib/foo.rb should be found — no non-.rb files scanned
        assert_eq!(results.len(), 0); // foo.rb has no operators, so 0 mutations
    }
}
