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

pub fn run_tests(project_path: &str) -> anyhow::Result<bool> {
    match detect_framework(project_path) {
        Framework::RSpec => {
            let status = Command::new("bundle")
                .args(["exec", "rspec", "--format", "progress"])
                .current_dir(project_path)
                .status()?;
            Ok(status.success())
        }
        Framework::Minitest => {
            let status = Command::new("bundle")
                .args(["exec", "rake", "test"])
                .current_dir(project_path)
                .status()?;
            Ok(status.success())
        }
        Framework::Unknown => {
            anyhow::bail!("No test framework detected (no spec/ or test/ directory)")
        }
    }
}
