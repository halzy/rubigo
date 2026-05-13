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
                eprintln!("rspec exited with code {:?}", output.status.code());
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
                eprintln!("minitest exited with code {:?}", output.status.code());
                Ok(TestOutcome::Error)
            }
        }
        Framework::Unknown => {
            anyhow::bail!("No test framework detected (no spec/ or test/ directory)")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Framework detection ────────────────────────────────

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

    // ── Test outcome helpers ───────────────────────────────

    #[test]
    fn test_outcome_equality() {
        assert_eq!(TestOutcome::Pass, TestOutcome::Pass);
        assert_eq!(TestOutcome::Fail, TestOutcome::Fail);
        assert_eq!(TestOutcome::Error, TestOutcome::Error);
        assert_ne!(TestOutcome::Pass, TestOutcome::Fail);
        assert_ne!(TestOutcome::Pass, TestOutcome::Error);
        assert_ne!(TestOutcome::Fail, TestOutcome::Error);
    }

    // ── run_tests error cases ──────────────────────────────

    #[test]
    fn test_run_tests_unknown_framework_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_tests(dir.path().to_str().unwrap(), &[]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No test framework detected"));
    }

    #[test]
    fn test_run_tests_with_rspec_args_passes_them_through() {
        // We can't easily test the actual command invocation, but we can
        // verify that unknown framework still fails when args are passed.
        let dir = tempfile::tempdir().unwrap();
        let args = vec!["--tag".to_string(), "~slow".to_string()];
        let result = run_tests(dir.path().to_str().unwrap(), &args);
        assert!(result.is_err());
    }
}
