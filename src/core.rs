use crate::mutator::MutationPoint;
use crate::parser;
use crate::runner::{self, TestOutcome};

#[derive(Debug)]
pub enum MutationOutcome {
    Killed,     // tests ran and caught the mutation
    Survived,   // tests ran but didn't catch it
    Error,      // tests could not run (infrastructure, etc.)
}

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
pub fn run_mutation_testing(
    project_path: &str,
    rspec_args: &[String],
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

    // Step 3: Run baseline test suite first to ensure it works
    println!("Running baseline test suite...");
    let baseline = runner::run_tests(project_path, rspec_args)?;
    if baseline == TestOutcome::Error {
        eprintln!("WARNING: Baseline test suite could not run — all mutations will report as errors.\n");
    }

    // Step 4: Test each mutation one at a time
    let mut results = Vec::new();
    let total = all_points.len();
    let mut errors = 0usize;

    for (i, point) in all_points.iter().enumerate() {
        println!(
            "[{}/{}] Testing {} ({} -> {}) at bytes {}-{}",
            i + 1,
            total,
            point.file,
            point.original,
            point.replacement,
            point.start_byte,
            point.end_byte
        );

        // Read, mutate, write in-place, test, restore
        let original = std::fs::read_to_string(&point.file)?;
        let mutated = crate::mutator::apply_mutation(&original, point);
        std::fs::write(&point.file, &mutated)?;

        let outcome = runner::run_tests(project_path, rspec_args).unwrap_or(TestOutcome::Error);

        // Restore original
        std::fs::write(&point.file, &original)?;

        let mutation_outcome = match outcome {
            TestOutcome::Pass => MutationOutcome::Survived,
            TestOutcome::Fail => MutationOutcome::Killed,
            TestOutcome::Error => {
                errors += 1;
                MutationOutcome::Error
            }
        };

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
