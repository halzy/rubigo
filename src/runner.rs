use std::process::Command;

pub enum Framework {
    RSpec,
    Minitest,
    Unknown,
}

pub fn detect_framework(project_path: &str) -> Framework {
    let path = std::path::Path::new(project_path);
    if path.join("spec").is_dir() {
        Framework::RSpec
    } else if path.join("test").is_dir() {
        Framework::Minitest
    } else {
        Framework::Unknown
    }
}

/// Derive the spec file path for a source file.
///
/// Conventions:
///   `app/models/user.rb`     → `spec/models/user_spec.rb`
///   `lib/foo/bar.rb`         → `spec/lib/foo/bar_spec.rb`
///   `app/controllers/a.rb`   → `spec/controllers/a_spec.rb`
///
/// Returns `None` if the derived spec file does not exist on disk.
pub fn derive_spec_file(source_file: &str, project_path: &str) -> Option<String> {
    let rel = source_file.strip_prefix(project_path)?.trim_start_matches('/');

    // Strip `app/` prefix if present; keep `lib/` prefix.
    let spec_rel = if let Some(rest) = rel.strip_prefix("app/") {
        format!("spec/{}", rest)
    } else {
        format!("spec/{}", rel)
    };

    // Replace .rb suffix with _spec.rb
    let spec_rel = format!("{}_spec.rb", spec_rel.strip_suffix(".rb")?);

    let full_path = std::path::Path::new(project_path).join(&spec_rel);
    if full_path.exists() {
        Some(full_path.to_string_lossy().to_string())
    } else {
        None
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

/// Run the test suite.
///
/// If `test_cmd` is provided, it is executed verbatim via `sh -c` after
/// substituting `{spec_file}` with the optional spec file path. Supports
/// env vars, pipes, etc.
///
/// Otherwise, the test framework is auto-detected and the default command
/// is used. For RSpec, `spec_file` targets a single spec when available.
pub fn run_tests(
    project_path: &str,
    test_cmd: Option<&str>,
    spec_file: Option<&str>,
) -> anyhow::Result<TestRun> {
    let output = if let Some(cmd) = test_cmd {
        // Template substitution: {spec_file} → spec_file or empty string
        let cmd = if let Some(spec) = spec_file {
            cmd.replace("{spec_file}", spec)
        } else {
            cmd.replace("{spec_file}", "")
        };
        Command::new("sh")
            .args(["-c", &cmd])
            .current_dir(project_path)
            .output()?
    } else {
        match detect_framework(project_path) {
            Framework::RSpec => {
                let mut cmd = Command::new("bundle");
                cmd.args(["exec", "rspec", "--format", "progress"]);
                if let Some(spec) = spec_file {
                    cmd.arg(spec);
                }
                cmd.current_dir(project_path).output()?
            }
            Framework::Minitest => {
                // Minitest has no clean 1:1 file→test mapping, always run full suite
                Command::new("bundle")
                    .args(["exec", "rake", "test"])
                    .current_dir(project_path)
                    .output()?
            }
            Framework::Unknown => {
                anyhow::bail!("No test framework detected. Provide --test-cmd.")
            }
        }
    };

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

#[cfg(test)]
mod tests {
    use super::*;

    // ── Framework detection ──────────────────────────────

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

    // ── derive_spec_file ─────────────────────────────────

    #[test]
    fn test_derive_spec_file_app_models() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec/models")).unwrap();
        std::fs::write(dir.path().join("spec/models/user_spec.rb"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("app/models")).unwrap();
        std::fs::write(dir.path().join("app/models/user.rb"), "class User; end").unwrap();

        let project_path = dir.path().to_str().unwrap();
        let source = dir.path().join("app/models/user.rb");
        let result = derive_spec_file(&source.to_string_lossy(), project_path);
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("spec/models/user_spec.rb"));
    }

    #[test]
    fn test_derive_spec_file_lib() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec/lib/foo")).unwrap();
        std::fs::write(dir.path().join("spec/lib/foo/bar_spec.rb"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("lib/foo")).unwrap();
        std::fs::write(dir.path().join("lib/foo/bar.rb"), "class Bar; end").unwrap();

        let project_path = dir.path().to_str().unwrap();
        let source = dir.path().join("lib/foo/bar.rb");
        let result = derive_spec_file(&source.to_string_lossy(), project_path);
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("spec/lib/foo/bar_spec.rb"));
    }

    #[test]
    fn test_derive_spec_file_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        std::fs::write(dir.path().join("lib/no_spec.rb"), "# no spec file").unwrap();

        let project_path = dir.path().to_str().unwrap();
        let source = dir.path().join("lib/no_spec.rb");
        let result = derive_spec_file(&source.to_string_lossy(), project_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_derive_spec_file_outside_project() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().to_str().unwrap();
        let result = derive_spec_file("/some/other/path/foo.rb", project_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_derive_spec_file_deeply_nested_lib() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec/lib/a/b/c/d")).unwrap();
        std::fs::write(
            dir.path().join("spec/lib/a/b/c/d/deep_spec.rb"),
            "",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("lib/a/b/c/d")).unwrap();
        std::fs::write(
            dir.path().join("lib/a/b/c/d/deep.rb"),
            "class Deep; end",
        )
        .unwrap();

        let project_path = dir.path().to_str().unwrap();
        let source = dir.path().join("lib/a/b/c/d/deep.rb");
        let result = derive_spec_file(&source.to_string_lossy(), project_path);
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("spec/lib/a/b/c/d/deep_spec.rb"));
    }

    #[test]
    fn test_derive_spec_file_app_controllers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec/controllers/api/v1")).unwrap();
        std::fs::write(
            dir.path().join("spec/controllers/api/v1/users_controller_spec.rb"),
            "",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("app/controllers/api/v1")).unwrap();
        std::fs::write(
            dir.path().join("app/controllers/api/v1/users_controller.rb"),
            "class UsersController; end",
        )
        .unwrap();

        let project_path = dir.path().to_str().unwrap();
        let source = dir.path().join("app/controllers/api/v1/users_controller.rb");
        let result = derive_spec_file(&source.to_string_lossy(), project_path);
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("spec/controllers/api/v1/users_controller_spec.rb"));
    }

    #[test]
    fn test_derive_spec_file_non_rb_not_matched() {
        // A .rb file must end in .rb or there's nothing to strip
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec/lib")).unwrap();
        std::fs::write(dir.path().join("spec/lib/template_spec.rb.erb"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("lib")).unwrap();
        std::fs::write(
            dir.path().join("lib/template.rb.erb"),
            "# ERB template",
        )
        .unwrap();

        let project_path = dir.path().to_str().unwrap();
        let source = dir.path().join("lib/template.rb.erb");
        let result = derive_spec_file(&source.to_string_lossy(), project_path);
        assert!(result.is_none()); // strip_suffix(".rb") fails on .rb.erb
    }

    #[test]
    fn test_derive_spec_file_spec_dir_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("app/models")).unwrap();
        std::fs::write(dir.path().join("app/models/user.rb"), "class User; end").unwrap();
        // No spec/ directory at all

        let project_path = dir.path().to_str().unwrap();
        let source = dir.path().join("app/models/user.rb");
        let result = derive_spec_file(&source.to_string_lossy(), project_path);
        assert!(result.is_none()); // derived path doesn't exist on disk
    }

    #[test]
    fn test_derive_spec_file_spec_file_not_created() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec/models")).unwrap();
        // spec/models/ dir exists but no _spec.rb inside
        std::fs::create_dir_all(dir.path().join("app/models")).unwrap();
        std::fs::write(dir.path().join("app/models/user.rb"), "class User; end").unwrap();

        let project_path = dir.path().to_str().unwrap();
        let source = dir.path().join("app/models/user.rb");
        let result = derive_spec_file(&source.to_string_lossy(), project_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_derive_spec_file_strips_only_app_prefix() {
        // lib/foo/bar.rb should become spec/lib/foo/bar_spec.rb (not spec/foo/bar_spec.rb)
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spec/lib/foo")).unwrap();
        std::fs::write(dir.path().join("spec/lib/foo/bar_spec.rb"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("lib/foo")).unwrap();
        std::fs::write(dir.path().join("lib/foo/bar.rb"), "class Bar; end").unwrap();

        let project_path = dir.path().to_str().unwrap();
        let source = dir.path().join("lib/foo/bar.rb");
        let result = derive_spec_file(&source.to_string_lossy(), project_path);
        assert!(result.is_some());
        // Should be spec/lib/..., not spec/foo/...
        assert!(result.unwrap().ends_with("spec/lib/foo/bar_spec.rb"));
    }

    // ── run_tests ────────────────────────────────────────

    #[test]
    fn test_run_tests_unknown_framework_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_tests(dir.path().to_str().unwrap(), None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_test_cmd_pass() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_tests(dir.path().to_str().unwrap(), Some("echo ok && exit 0"), None);
        assert_eq!(result.unwrap().outcome, TestOutcome::Pass);
    }

    #[test]
    fn test_custom_test_cmd_fail() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_tests(dir.path().to_str().unwrap(), Some("echo fail && exit 1"), None);
        assert_eq!(result.unwrap().outcome, TestOutcome::Fail);
    }

    #[test]
    fn test_custom_test_cmd_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_tests(dir.path().to_str().unwrap(), Some("exit 2"), None);
        assert_eq!(result.unwrap().outcome, TestOutcome::Error);
    }

    #[test]
    fn test_template_substitution_with_spec_file() {
        let dir = tempfile::tempdir().unwrap();
        // Command echoes the spec file path; exit 1 to simulate test failure
        let result = run_tests(
            dir.path().to_str().unwrap(),
            Some("echo {spec_file} && exit 1"),
            Some("spec/models/user_spec.rb"),
        );
        let run = result.unwrap();
        assert_eq!(run.outcome, TestOutcome::Fail);
        assert!(run.stdout.contains("spec/models/user_spec.rb"));
    }

    #[test]
    fn test_template_substitution_without_spec_file() {
        let dir = tempfile::tempdir().unwrap();
        // {spec_file} should be replaced with empty string
        let result = run_tests(
            dir.path().to_str().unwrap(),
            Some("echo [{spec_file}] && exit 0"),
            None,
        );
        let run = result.unwrap();
        assert_eq!(run.outcome, TestOutcome::Pass);
        assert!(run.stdout.contains("[]"));
    }

    #[test]
    fn test_template_no_placeholder_passes_through() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_tests(
            dir.path().to_str().unwrap(),
            Some("echo hello && exit 0"),
            Some("spec/something_spec.rb"),
        );
        let run = result.unwrap();
        assert_eq!(run.outcome, TestOutcome::Pass);
        assert!(run.stdout.contains("hello"));
    }

    #[test]
    fn test_multiple_template_placeholders() {
        // {spec_file} should be replaced at every occurrence
        let dir = tempfile::tempdir().unwrap();
        let result = run_tests(
            dir.path().to_str().unwrap(),
            Some("echo {spec_file} {spec_file} && exit 1"),
            Some("spec/models/user_spec.rb"),
        );
        let run = result.unwrap();
        assert_eq!(run.outcome, TestOutcome::Fail);
        // stdout should contain the spec path twice
        let count = run
            .stdout
            .matches("spec/models/user_spec.rb")
            .count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_template_with_spaces_in_spec_path() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_tests(
            dir.path().to_str().unwrap(),
            Some("echo [{spec_file}] && exit 1"),
            Some("spec/path with spaces/my_spec.rb"),
        );
        let run = result.unwrap();
        // Template substitution is a literal string replace — caller should
        // ensure the spec path is passed correctly through sh -c.
        assert!(run.stdout.contains("spec/path with spaces/my_spec.rb"));
    }

    #[test]
    fn test_exit_code_captured() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_tests(
            dir.path().to_str().unwrap(),
            Some("exit 0"),
            None,
        );
        let run = result.unwrap();
        assert_eq!(run.exit_code, Some(0));

        let result = run_tests(
            dir.path().to_str().unwrap(),
            Some("exit 1"),
            None,
        );
        let run = result.unwrap();
        assert_eq!(run.exit_code, Some(1));

        let result = run_tests(
            dir.path().to_str().unwrap(),
            Some("exit 42"),
            None,
        );
        let run = result.unwrap();
        assert_eq!(run.exit_code, Some(42));
    }

    #[test]
    fn test_stderr_captured() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_tests(
            dir.path().to_str().unwrap(),
            Some("echo stdout && echo stderr >&2 && exit 1"),
            None,
        );
        let run = result.unwrap();
        assert!(run.stdout.contains("stdout"));
        assert!(run.stderr.contains("stderr"));
    }
}
