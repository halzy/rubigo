use std::time::Instant;

use tree_sitter::Tree;

use crate::cache::{load_cache, save_cache, MutationId};
use crate::config::Config;
use crate::emit;
use crate::io::FileGuard;
use crate::mutation::MutationPoint;
use crate::parser;
use crate::runner::{self, TestRun};

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

/// Run mutation testing on a Ruby project using the given config.
pub fn run_mutation_testing(cfg: &Config) -> anyhow::Result<Vec<MutationResult>> {
    let project_path = cfg.project_path;

    // Step 1: Find Ruby source files
    let rb_files: Vec<String> = walkdir::WalkDir::new(project_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rb"))
        .filter(|e| {
            let p = e.path().to_string_lossy();
            !p.contains("/spec/") && !p.contains("/test/") && !p.contains("/vendor/")
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
        let points = parser::find_mutations(&tree, &source, file);
        if !points.is_empty() {
            file_trees.insert(file.clone(), FileTree { source, tree });
        }
        all_points.extend(points);
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

    // Step 2.5: Cache
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
            if cfg.verbosity.show_on_failure() {
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

    // Step 3: Baseline
    println!("Running baseline test suite...");
    let baseline_start = Instant::now();
    let baseline = runner::run_tests(project_path, cfg.test_cmd)?;
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

    // Step 4: Test each mutation
    let total = all_points.len();
    let start_time = Instant::now();
    let mut newly_killed: Vec<MutationId> = Vec::new();

    for (i, point) in all_points.iter().enumerate() {
        let ft = &file_trees[&point.file];
        let mutated = emit::emit_tree(&ft.tree, &ft.source, point.node_id, &point.replacement);

        let mut guard = FileGuard::overwrite(std::path::Path::new(&point.file), &mutated)?;

        let test_run =
            runner::run_tests(project_path, cfg.test_cmd).unwrap_or_else(|_| TestRun {
                outcome: runner::TestOutcome::Error,
                stdout: String::new(),
                stderr: "run_tests failed".into(),
                exit_code: None,
            });

        guard.restore()?;

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

        let outcome_str = match mutation_outcome {
            MutationOutcome::Killed => "KILLED",
            MutationOutcome::Survived => "SURVIVED",
            MutationOutcome::Error => "ERROR",
            MutationOutcome::Skipped => "SKIPPED",
        };

        println!(
            "[{}/{}] {}:{}  {} -> {}  [{}]  est. remaining: ~{:?}",
            done,
            total,
            point.file,
            point.line_number,
            point.original,
            point.replacement,
            outcome_str,
            eta,
        );

        let show = mutation_outcome == MutationOutcome::Survived
            || mutation_outcome == MutationOutcome::Error;
        if (show && cfg.verbosity.show_on_failure()) || cfg.verbosity.show_always() {
            println!("{}", test_run.stdout);
            if !test_run.stderr.is_empty() {
                eprintln!("{}", test_run.stderr);
            }
        }

        results.push(MutationResult {
            point: point.clone(),
            outcome: mutation_outcome,
        });
    }

    // Save cache once at the end (batched)
    if let Some((_, ref cache_path)) = cache {
        if !newly_killed.is_empty() {
            save_cache(cache_path, &newly_killed);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Verbosity;

    fn make_point(file: &str, line: usize, node_id: usize, original: &str, replacement: &str) -> MutationPoint {
        MutationPoint {
            file: file.to_string(),
            line_number: line,
            node_id,
            original: original.to_string(),
            replacement: replacement.to_string(),
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
    fn test_rejects_non_ruby() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: None,
            cache_path: None,
            limit: None,
            verbosity: Verbosity::Quiet,
        };
        let result = run_mutation_testing(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No .rb source files"));
    }

    #[test]
    fn test_empty_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        std::fs::write(dir.path().join("lib").join("foo.rb"), "# nothing\n").unwrap();
        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: None,
            cache_path: None,
            limit: None,
            verbosity: Verbosity::Quiet,
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
        ).unwrap();
        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: None,
            cache_path: None,
            limit: Some(0),
            verbosity: Verbosity::Quiet,
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
                file: dir.path().join("lib/foo.rb").to_string_lossy().to_string(),
                line_number: 3,
                original: "==".to_string(),
            }],
        );

        let cfg = Config {
            project_path: dir.path().to_str().unwrap(),
            test_cmd: None,
            cache_path: Some(cache_path.to_str().unwrap()),
            limit: None,
            verbosity: Verbosity::Quiet,
        };
        let results = run_mutation_testing(&cfg).unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().all(|r| r.skipped()));
    }
}
