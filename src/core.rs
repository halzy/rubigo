use std::time::Instant;

use crate::mutator::MutationPoint;
use crate::parser;
use crate::runner::{self, TestRun};

#[derive(Debug, PartialEq)]
pub enum MutationOutcome {
    Killed,   // tests ran and caught the mutation
    Survived, // tests ran but didn't catch it
    Error,    // tests could not run (infrastructure, etc.)
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
}

/// Run mutation testing on a Ruby project directory.
/// `verbosity`: 0 = default, 1 = show rspec output on failure, 2+ = always show rspec output.
pub fn run_mutation_testing(
    project_path: &str,
    rspec_args: &[String],
    limit: Option<usize>,
    verbosity: u8,
) -> anyhow::Result<Vec<MutationResult>> {
    // Step 1: Find all Ruby source files (exclude spec/, test/, vendor/ dirs)
    let rb_files: Vec<String> = walkdir::WalkDir::new(project_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rb"))
        .filter(|e| {
            let path_str = e.path().to_string_lossy();
            !path_str.contains("/spec/")
                && !path_str.contains("/test/")
                && !path_str.contains("/vendor/")
        })
        .map(|e| e.path().to_string_lossy().to_string())
        .collect();

    if rb_files.is_empty() {
        anyhow::bail!("No .rb source files found in {}", project_path);
    }

    // Step 2: Collect all mutation points from all files
    let mut all_points: Vec<MutationPoint> = Vec::new();
    for file in &rb_files {
        let source = std::fs::read_to_string(file)?;
        let tree = parser::parse_source(&source)?;
        let points = parser::find_eq_mutations(&tree, &source, file);
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

    // Apply mutation limit if set
    if let Some(n) = limit {
        if n < all_points.len() {
            all_points.truncate(n);
            println!("Limited to first {} mutation(s)\n", n);
        }
    }

    // Step 3: Run baseline test suite first to time it and ensure it works
    println!("Running baseline test suite...");
    let baseline_start = Instant::now();
    let baseline = runner::run_tests(project_path, rspec_args)?;
    let baseline_duration = baseline_start.elapsed();

    if verbosity >= 2 {
        println!("--- baseline rspec output ---");
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
            baseline_duration, total_est, all_points.len()
        );
    }

    // Step 4: Test each mutation one at a time
    let mut results = Vec::new();
    let total = all_points.len();
    let mut errors = 0usize;
    let start_time = Instant::now();

    for (i, point) in all_points.iter().enumerate() {
        // Read, mutate, write in-place, test, restore
        let original = std::fs::read_to_string(&point.file)?;
        let mutated = crate::mutator::apply_mutation(&original, point);
        std::fs::write(&point.file, &mutated)?;

        let test_run = runner::run_tests(project_path, rspec_args).unwrap_or_else(|_| TestRun {
            outcome: runner::TestOutcome::Error,
            stdout: String::new(),
            stderr: "run_tests returned Err".into(),
            exit_code: None,
        });

        // Restore original
        std::fs::write(&point.file, &original)?;

        let mutation_outcome = match test_run.outcome {
            runner::TestOutcome::Pass => MutationOutcome::Survived,
            runner::TestOutcome::Fail => MutationOutcome::Killed,
            runner::TestOutcome::Error => {
                errors += 1;
                MutationOutcome::Error
            }
        };

        // ETA calculation
        let elapsed = start_time.elapsed();
        let done = i + 1;
        let remaining = total - done;
        let avg_per = elapsed / done as u32;
        let eta = avg_per * remaining as u32;

        let outcome_str = match mutation_outcome {
            MutationOutcome::Killed => "KILLED",
            MutationOutcome::Survived => "SURVIVED",
            MutationOutcome::Error => "ERROR",
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

        // Verbosity: show test output
        if matches!(mutation_outcome, MutationOutcome::Survived | MutationOutcome::Error)
            && verbosity >= 1
            || verbosity >= 2
        {
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

    if errors > 0 {
        eprintln!(
            "WARNING: {} mutation(s) could not be tested due to test suite errors.",
            errors
        );
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_point(
        file: &str,
        line: usize,
        start: usize,
        end: usize,
        original: &str,
        replacement: &str,
    ) -> MutationPoint {
        MutationPoint {
            file: file.to_string(),
            line_number: line,
            start_byte: start,
            end_byte: end,
            original: original.to_string(),
            replacement: replacement.to_string(),
        }
    }

    #[test]
    fn test_killed_returns_true_for_killed() {
        let r = MutationResult {
            point: make_point("a.rb", 1, 0, 2, "==", "!="),
            outcome: MutationOutcome::Killed,
        };
        assert!(r.killed());
        assert!(!r.survived());
        assert!(!r.errored());
    }

    #[test]
    fn test_survived_returns_true_for_survived() {
        let r = MutationResult {
            point: make_point("a.rb", 1, 0, 2, "==", "!="),
            outcome: MutationOutcome::Survived,
        };
        assert!(!r.killed());
        assert!(r.survived());
        assert!(!r.errored());
    }

    #[test]
    fn test_errored_returns_true_for_error() {
        let r = MutationResult {
            point: make_point("a.rb", 1, 0, 2, "==", "!="),
            outcome: MutationOutcome::Error,
        };
        assert!(!r.killed());
        assert!(!r.survived());
        assert!(r.errored());
    }

    #[test]
    fn test_mutation_outcomes_are_mutually_exclusive() {
        let killed = MutationResult {
            point: make_point("a.rb", 1, 0, 2, "==", "!="),
            outcome: MutationOutcome::Killed,
        };
        let survived = MutationResult {
            point: make_point("b.rb", 1, 0, 2, "!=", "=="),
            outcome: MutationOutcome::Survived,
        };
        let errored = MutationResult {
            point: make_point("c.rb", 1, 0, 2, "==", "!="),
            outcome: MutationOutcome::Error,
        };

        assert!(killed.killed() && !killed.survived() && !killed.errored());
        assert!(!survived.killed() && survived.survived() && !survived.errored());
        assert!(!errored.killed() && !errored.survived() && errored.errored());
    }

    #[test]
    fn test_mutation_outcome_equality() {
        assert_eq!(MutationOutcome::Killed, MutationOutcome::Killed);
        assert_eq!(MutationOutcome::Survived, MutationOutcome::Survived);
        assert_eq!(MutationOutcome::Error, MutationOutcome::Error);
        assert_ne!(MutationOutcome::Killed, MutationOutcome::Survived);
        assert_ne!(MutationOutcome::Killed, MutationOutcome::Error);
        assert_ne!(MutationOutcome::Survived, MutationOutcome::Error);
    }

    #[test]
    fn test_run_mutation_testing_rejects_non_ruby_projects() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_mutation_testing(dir.path().to_str().unwrap(), &[], None, 0);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No .rb source files"));
    }

    #[test]
    fn test_run_mutation_testing_empty_project_no_mutations() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec")).unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        std::fs::write(dir.path().join("lib").join("foo.rb"), "# nothing\n").unwrap();
        let _ = run_mutation_testing(dir.path().to_str().unwrap(), &[], None, 0);
    }
}
