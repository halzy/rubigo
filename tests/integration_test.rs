mod common;

use rubigo::config::{Config, Verbosity};

fn make_cfg(test_dir: &tempfile::TempDir) -> Config {
    Config {
        project_path: test_dir.path().to_str().unwrap(),
        test_cmd: None,
        cache_path: None,
        limit: None,
        list_only: false,
        verbosity: Verbosity::Quiet,
    }
}

#[test]
fn test_eq_mutation_killed_when_spec_covers_both_cases() {
    let Some((_ruby_dir, bundle_bin)) = common::discover_ruby() else {
        eprintln!("SKIP: no working Ruby + Bundler found");
        return;
    };
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
    common::scaffold_ruby_project(dir.path(), &bundle_bin, "truth", source, spec);

    let results = rubigo::core::run_mutation_testing(&make_cfg(&dir)).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].killed());
    assert_eq!(results[0].point.original, "==");
    assert_eq!(results[0].point.replacement, "!=");
}

#[test]
fn test_no_mutations_found_for_addition_only() {
    let Some((_ruby_dir, bundle_bin)) = common::discover_ruby() else {
        eprintln!("SKIP: no working Ruby + Bundler found");
        return;
    };
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
    common::scaffold_ruby_project(dir.path(), &bundle_bin, "calc", source, spec);

    let results = rubigo::core::run_mutation_testing(&make_cfg(&dir)).unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_multiple_eq_mutations_all_killed() {
    let Some((_ruby_dir, bundle_bin)) = common::discover_ruby() else {
        eprintln!("SKIP: no working Ruby + Bundler found");
        return;
    };
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
    common::scaffold_ruby_project(dir.path(), &bundle_bin, "chk", source, spec);

    let results = rubigo::core::run_mutation_testing(&make_cfg(&dir)).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.killed()));
}

#[test]
fn test_neq_mutation_killed() {
    let Some((_ruby_dir, bundle_bin)) = common::discover_ruby() else {
        eprintln!("SKIP: no working Ruby + Bundler found");
        return;
    };
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
    common::scaffold_ruby_project(dir.path(), &bundle_bin, "checker", source, spec);

    let results = rubigo::core::run_mutation_testing(&make_cfg(&dir)).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].killed());
}

#[test]
fn test_two_files_each_gets_targeted_spec() {
    // Two source files, each with mutations and a corresponding _spec.rb.
    // Using a custom test-cmd that writes the {spec_file} to a marker file
    // so we can verify which spec was targeted.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("spec/lib")).unwrap();
    std::fs::create_dir_all(dir.path().join("lib")).unwrap();

    // File A with one mutation
    std::fs::write(
        dir.path().join("lib").join("a.rb"),
        "class A\n  def eq?(a, b)\n    a == b\n  end\nend\n",
    )
    .unwrap();

    // File B with one mutation
    std::fs::write(
        dir.path().join("lib").join("b.rb"),
        "class B\n  def eq?(a, b)\n    a == b\n  end\nend\n",
    )
    .unwrap();

    // Spec A — writes a marker file then exits 1 (fail = killed)
    std::fs::write(
        dir.path().join("spec/lib/a_spec.rb"),
        "File.write('spec_lib_a_spec_marker', 'ran'); exit 1\n",
    )
    .unwrap();

    // Spec B — writes a different marker file
    std::fs::write(
        dir.path().join("spec/lib/b_spec.rb"),
        "File.write('spec_lib_b_spec_marker', 'ran'); exit 1\n",
    )
    .unwrap();

    // Clean up any leftover markers from a previous run
    let marker_a = dir.path().join("spec_lib_a_spec_marker");
    let marker_b = dir.path().join("spec_lib_b_spec_marker");
    let _ = std::fs::remove_file(&marker_a);
    let _ = std::fs::remove_file(&marker_b);

    let cfg = Config {
        project_path: dir.path().to_str().unwrap(),
        test_cmd: Some("ruby {spec_file}"),
        cache_path: None,
        limit: None,
        list_only: false,
        verbosity: Verbosity::Quiet,
    };

    let results = rubigo::core::run_mutation_testing(&cfg).unwrap();
    assert_eq!(results.len(), 2);
    assert!(
        results.iter().all(|r| r.killed()),
        "all mutations should be killed"
    );

    // Both markers should exist — proves each spec was targeted individually
    assert!(marker_a.exists(), "spec_lib_a_spec_marker should exist");
    assert!(marker_b.exists(), "spec_lib_b_spec_marker should exist");

    // Cleanup
    let _ = std::fs::remove_file(&marker_a);
    let _ = std::fs::remove_file(&marker_b);
}
