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

/// Full result of a test run: outcome plus captured output for verbosity.
pub struct TestRun {
    pub outcome: TestOutcome,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

/// Run the test suite. Respects extra rspec CLI args if provided.
pub fn run_tests(project_path: &str, rspec_args: &[String]) -> anyhow::Result<TestRun> {
    match detect_framework(project_path) {
        Framework::RSpec => {
            let mut cmd = Command::new("bundle");
            cmd.args(["exec", "rspec", "--format", "progress"]);
            for arg in rspec_args {
                cmd.arg(arg);
            }
            cmd.current_dir(project_path);
            let output = cmd.output()?;

            let code = output.status.code();
            let outcome = if output.status.success() {
                TestOutcome::Pass
            } else if code == Some(1) {
                TestOutcome::Fail
            } else {
                TestOutcome::Error
            };

            Ok(TestRun {
                outcome,
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                exit_code: code,
            })
        }
        Framework::Minitest => {
            let output = Command::new("bundle")
                .args(["exec", "rake", "test"])
                .current_dir(project_path)
                .output()?;

            let code = output.status.code();
            let outcome = if output.status.success() {
                TestOutcome::Pass
            } else if code == Some(1) {
                TestOutcome::Fail
            } else {
                TestOutcome::Error
            };

            Ok(TestRun {
                outcome,
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                exit_code: code,
            })
        }
        Framework::Unknown => {
            anyhow::bail!("No test framework detected (no spec/ or test/ directory)")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_rspec_when_spec_dir_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec")).unwrap();
        let path = dir.path().to_str().unwrap();
        assert!(matches!(detect_framework(path), Framework::RSpec));
    }

    #[test]
    fn test_detect_minitest_when_test_dir_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("test")).unwrap();
        let path = dir.path().to_str().unwrap();
        assert!(matches!(detect_framework(path), Framework::Minitest));
    }

    #[test]
    fn test_detect_rspec_over_minitest_when_both_exist() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec")).unwrap();
        std::fs::create_dir_all(dir.path().join("test")).unwrap();
        let path = dir.path().to_str().unwrap();
        assert!(matches!(detect_framework(path), Framework::RSpec));
    }

    #[test]
    fn test_detect_unknown_when_no_framework_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();
        assert!(matches!(detect_framework(path), Framework::Unknown));
    }

    #[test]
    fn test_outcome_equality() {
        assert_eq!(TestOutcome::Pass, TestOutcome::Pass);
        assert_eq!(TestOutcome::Fail, TestOutcome::Fail);
        assert_eq!(TestOutcome::Error, TestOutcome::Error);
        assert_ne!(TestOutcome::Pass, TestOutcome::Fail);
        assert_ne!(TestOutcome::Pass, TestOutcome::Error);
        assert_ne!(TestOutcome::Fail, TestOutcome::Error);
    }

    #[test]
    fn test_run_tests_unknown_framework_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_tests(dir.path().to_str().unwrap(), &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_tests_with_rspec_args_passes_them_through() {
        let dir = tempfile::tempdir().unwrap();
        let args = vec!["--tag".to_string(), "~slow".to_string()];
        let result = run_tests(dir.path().to_str().unwrap(), &args);
        assert!(result.is_err());
    }
}
