use std::fs;
use std::path::Path;
use std::process::Command;

/// Path to the rbenv Ruby 3.2.2 installation. The system Ruby 2.6 has
/// Bundler 1.17 which can't resolve modern RSpec gems.
const RBENV_RUBY_322: &str = "/Users/bhalsted/.rbenv/versions/3.2.2";

/// Returns the rbenv ruby path if it exists.
fn ruby_bin() -> &'static str {
    RBENV_RUBY_322
}

/// Check if the rbenv Ruby 3.2.2 is available.
fn ruby_available() -> bool {
    Path::new(RBENV_RUBY_322).join("bin/ruby").exists()
}

/// Scaffold a minimal Ruby project in `dir` with the given source and spec.
/// Creates Gemfile, runs `bundle install`, and creates spec/lib directories.
fn scaffold_ruby_project(dir: &Path, lib_name: &str, source: &str, spec: &str) {
    let lib_dir = dir.join("lib");
    let spec_dir = dir.join("spec");
    fs::create_dir_all(&lib_dir).unwrap();
    fs::create_dir_all(&spec_dir).unwrap();

    let bindir = Path::new(ruby_bin()).join("bin");
    let bundle = |args: &[&str]| {
        let mut cmd = Command::new(bindir.join("bundle"));
        cmd.args(args)
            .env("BUNDLE_PATH", dir.join("vendor/bundle"))
            .current_dir(dir);
        cmd
    };

    // Gemfile
    let gemfile = dir.join("Gemfile");
    fs::write(
        &gemfile,
        r#"source "https://rubygems.org"
gem "rspec"
"#,
    )
    .unwrap();

    // Bundle install
    let status = bundle(&["install", "--quiet"]).status().expect("bundle install failed");
    assert!(status.success(), "bundle install exited with error");

    // Source file
    fs::write(lib_dir.join(format!("{}.rb", lib_name)), source).unwrap();

    // Spec file
    fs::write(spec_dir.join(format!("{}_spec.rb", lib_name)), spec).unwrap();

    // Verify tests pass
    let status = bundle(&["exec", "rspec", "--format", "progress"])
        .status()
        .unwrap();
    assert!(status.success(), "base test suite should pass before mutation");
}

/// Operator `==` in source. Spec tests both true and false cases.
/// Mutation `==` → `!=` should be killed.
#[test]
fn test_eq_mutation_killed_when_spec_covers_both_cases() {
    if !ruby_available() {
        eprintln!("SKIP: rbenv Ruby 3.2.2 not found");
        return;
    }

    let dir = tempfile::tempdir().unwrap();

    let source = "class Truth\n  def self.equal?(a, b)\n    a == b\n  end\nend\n";
    let spec = "require_relative '../lib/truth'

RSpec.describe Truth do
  describe '.equal?' do
    it 'returns true when equal' do
      expect(Truth.equal?(1, 1)).to be true
    end
    it 'returns false when not equal' do
      expect(Truth.equal?(1, 2)).to be false
    end
  end
end
";

    scaffold_ruby_project(dir.path(), "truth", source, spec);

    let results = ferox::core::run_mutation_testing(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(results.len(), 1, "should find exactly one mutation point");
    assert!(results[0].killed, "mutation should be killed when spec covers both cases");
    assert_eq!(results[0].point.original, "==");
    assert_eq!(results[0].point.replacement, "!=");
}

/// Project has no `==` or `!=` operators. Should find zero mutation points.
#[test]
fn test_no_mutations_found_for_addition_only() {
    if !ruby_available() {
        eprintln!("SKIP: rbenv Ruby 3.2.2 not found");
        return;
    }

    let dir = tempfile::tempdir().unwrap();

    let source = "class Calc\n  def self.add(a, b)\n    a + b\n  end\nend\n";
    let spec = "require_relative '../lib/calc'

RSpec.describe Calc do
  describe '.add' do
    it 'adds two numbers' do
      expect(Calc.add(2, 3)).to eq(5)
    end
  end
end
";

    scaffold_ruby_project(dir.path(), "calc", source, spec);

    let results = ferox::core::run_mutation_testing(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(results.len(), 0, "no ==/!= operators → zero mutation points");
}

/// Two `==` operators in source. Spec covers both. Both mutations should be killed.
#[test]
fn test_multiple_eq_mutations_all_killed() {
    if !ruby_available() {
        eprintln!("SKIP: rbenv Ruby 3.2.2 not found");
        return;
    }

    let dir = tempfile::tempdir().unwrap();

    let source = "class Chk\n  def self.both?(a, b)\n    a == b\n  end\n  def self.same?(a, b)\n    a == b\n  end\nend\n";
    let spec = "require_relative '../lib/chk'

RSpec.describe Chk do
  describe '.both?' do
    it 'returns true when equal' do
      expect(Chk.both?(1, 1)).to be true
    end
    it 'returns false when unequal' do
      expect(Chk.both?(1, 2)).to be false
    end
  end
  describe '.same?' do
    it 'returns true when equal' do
      expect(Chk.same?(1, 1)).to be true
    end
    it 'returns false when unequal' do
      expect(Chk.same?(1, 2)).to be false
    end
  end
end
";

    scaffold_ruby_project(dir.path(), "chk", source, spec);

    let results = ferox::core::run_mutation_testing(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(results.len(), 2, "should find two mutation points");
    assert!(results.iter().all(|r| r.killed), "both mutations should be killed");
}

/// `!=` operator in source. Spec covers the not-equal case.
/// Mutation `!=` → `==` should be killed.
#[test]
fn test_neq_mutation_killed() {
    if !ruby_available() {
        eprintln!("SKIP: rbenv Ruby 3.2.2 not found");
        return;
    }

    let dir = tempfile::tempdir().unwrap();

    let source = "class Checker\n  def self.not?(a, b)\n    a != b\n  end\nend\n";
    let spec = "require_relative '../lib/checker'

RSpec.describe Checker do
  describe '.not?' do
    it 'returns true when not equal' do
      expect(Checker.not?(1, 2)).to be true
    end
    it 'returns false when equal' do
      expect(Checker.not?(1, 1)).to be false
    end
  end
end
";

    scaffold_ruby_project(dir.path(), "checker", source, spec);

    let results = ferox::core::run_mutation_testing(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(results.len(), 1, "should find one != mutation point");
    assert!(results[0].killed, "!= → == mutation should be killed");
}
