use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Test helper — discovers a working Ruby + Bundler, or provides
/// a skip message. No hardcoded paths.

/// Discover a working Ruby + Bundler installation (>= Ruby 2.7).
pub fn discover_ruby() -> Option<(PathBuf, PathBuf)> {
    // Try rbenv: find a non-system ruby >= 2.7
    if let Ok(output) = Command::new("rbenv").args(["versions", "--bare"]).output() {
        let versions = String::from_utf8_lossy(&output.stdout);
        for line in versions.lines() {
            let v = line.trim();
            if v == "system" {
                continue;
            }
            // rbenv version dir is ~/.rbenv/versions/<version>
            let version_dir =
                PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/Users/bhalsted".into()))
                    .join(".rbenv/versions")
                    .join(v);
            let bundle = version_dir.join("bin/bundle");
            if bundle.exists() {
                let ruby = version_dir.join("bin/ruby");
                if ruby_version_ok(&ruby) && bundle_working(&bundle) {
                    return Some((version_dir.join("bin"), bundle));
                }
            }
        }
    }

    // Fallback: try common system paths but check version
    for ruby in &[
        "/opt/homebrew/opt/ruby/bin/ruby",
        "/usr/local/bin/ruby",
        "/usr/bin/ruby",
    ] {
        let ruby = Path::new(ruby);
        if ruby.exists() && ruby_version_ok(ruby) {
            if let Some(parent) = ruby.parent() {
                let bundle = parent.join("bundle");
                if bundle.exists() && bundle_working(&bundle) {
                    return Some((parent.to_path_buf(), bundle));
                }
            }
        }
    }

    None
}

fn ruby_version_ok(ruby: &Path) -> bool {
    let output = Command::new(ruby)
        .args(["-e", "puts RUBY_VERSION >= '2.7'"])
        .output();
    output
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "true")
        .unwrap_or(false)
}

fn bundle_working(bundle: &Path) -> bool {
    Command::new(bundle)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Scaffold a minimal Ruby project in `dir` with the given source and spec.
pub fn scaffold_ruby_project(
    dir: &Path,
    bundle_bin: &Path,
    lib_name: &str,
    source: &str,
    spec: &str,
) {
    let lib_dir = dir.join("lib");
    let spec_dir = dir.join("spec");
    fs::create_dir_all(&lib_dir).unwrap();
    fs::create_dir_all(&spec_dir).unwrap();

    let gemfile = dir.join("Gemfile");
    fs::write(
        &gemfile,
        r#"source "https://rubygems.org"
gem "rspec"
"#,
    )
    .unwrap();

    // Bundle install into local path to avoid sudo
    let status = Command::new(bundle_bin)
        .args(["install", "--quiet"])
        .env("BUNDLE_PATH", dir.join("vendor/bundle"))
        .current_dir(dir)
        .status()
        .expect("bundle install failed");
    assert!(status.success(), "bundle install exited with error");

    fs::write(lib_dir.join(format!("{}.rb", lib_name)), source).unwrap();
    fs::write(spec_dir.join(format!("{}_spec.rb", lib_name)), spec).unwrap();

    // Verify tests pass before mutation
    let status = Command::new(bundle_bin)
        .args(["exec", "rspec", "--format", "progress"])
        .env("BUNDLE_PATH", dir.join("vendor/bundle"))
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "base test suite should pass before mutation");
}
