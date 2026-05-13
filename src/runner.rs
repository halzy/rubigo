use std::path::Path;
use std::process::Command;

pub enum Framework {
    RSpec,
    Minitest,
    Unknown,
}

pub fn detect_framework(project_path: &str) -> Framework {
    let path = Path::new(project_path);
    if path.join("spec").is_dir() {
        Framework::RSpec
    } else if path.join("test").is_dir() {
        Framework::Minitest
    } else {
        Framework::Unknown
    }
}

#[derive(Debug, PartialEq)]
pub enum TestOutcome {
    Pass,   // tests ran, all passed (mutation survived)
    Fail,   // tests ran, at least one failed (mutation killed)
    Error,  // tests could not run (infrastructure, compile error, etc.)
}

/// Run the test suite. Respects extra rspec CLI args if provided.
pub fn run_tests(project_path: &str, rspec_args: &[String]) -> anyhow::Result<TestOutcome> {
    match detect_framework(project_path) {
        Framework::RSpec => {
            let mut cmd = Command::new("bundle");
            cmd.args(["exec", "rspec", "--format", "progress"]);
            for arg in rspec_args {
                cmd.arg(arg);
            }
            cmd.current_dir(project_path);
            let output = cmd.output()?;

            if output.status.success() {
                Ok(TestOutcome::Pass)
            } else if output.status.code() == Some(1) {
                // RSpec exits 1 on test failures — that's expected
                Ok(TestOutcome::Fail)
            } else {
                // Exit code 2+ means rspec itself errored (syntax, load error, etc.)
                eprintln!(
                    "rspec exited with code {:?}",
                    output.status.code()
                );
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    eprintln!("{}", stderr);
                }
                Ok(TestOutcome::Error)
            }
        }
        Framework::Minitest => {
            let output = Command::new("bundle")
                .args(["exec", "rake", "test"])
                .current_dir(project_path)
                .output()?;

            if output.status.success() {
                Ok(TestOutcome::Pass)
            } else if output.status.code() == Some(1) {
                Ok(TestOutcome::Fail)
            } else {
                eprintln!(
                    "minitest exited with code {:?}",
                    output.status.code()
                );
                Ok(TestOutcome::Error)
            }
        }
        Framework::Unknown => {
            anyhow::bail!("No test framework detected (no spec/ or test/ directory)")
        }
    }
}
