# Ferox — Ruby Mutation Testing Tool — Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** A Rust CLI that parses Ruby source with `tree-sitter-ruby`, finds `==` and `!=` operators, flips them, runs the project's existing test suite, and reports which mutations survive.

**Architecture:** tree-sitter CST (concrete syntax tree) parsing. Walk the tree to find `==`/`!=` nodes. For each mutation: copy file to temp, apply replacement at byte offset, run RSpec/Minitest, record result. Byte-offset-based mutation avoids line/column ambiguity.

**Tech Stack:** Rust, tree-sitter + tree-sitter-ruby, clap (CLI), walkdir (file discovery), anyhow (errors).

---

## Task 1: Scaffold project with tree-sitter dependencies

**Objective:** Set up Cargo.toml with tree-sitter + tree-sitter-ruby, create module stubs, verify it compiles.

**Files:**
- Modify: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/parser.rs`
- Create: `src/mutator.rs`
- Create: `src/runner.rs`
- Create: `src/report.rs`

**Step 1: Write Cargo.toml**

```toml
[package]
name = "ferox"
version = "0.1.0"
edition = "2021"

[dependencies]
tree-sitter = "0.26"
tree-sitter-ruby = "0.23"
clap = { version = "4", features = ["derive"] }
walkdir = "2"
anyhow = "1"
```

**Step 2: Write `src/main.rs`**

```rust
mod mutator;
mod parser;
mod report;
mod runner;

fn main() {
    println!("ferox — mutation testing for Ruby");
}
```

**Step 3: Create stub modules**

`src/parser.rs`:
```rust
use tree_sitter::Tree;

pub fn parse_source(source: &str) -> anyhow::Result<Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_ruby::LANGUAGE.into())?;
    let tree = parser.parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("failed to parse Ruby source"))?;
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_ruby() {
        let tree = parse_source("x = 1 + 2").expect("should parse");
        let root = tree.root_node();
        assert!(root.child_count() > 0, "should have child nodes");
    }
}
```

`src/mutator.rs`:
```rust
#[derive(Debug, Clone)]
pub struct MutationPoint {
    pub file: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub original: String,
    pub replacement: String,
}

/// Replace bytes [start_byte..end_byte] in source with `replacement`.
pub fn apply_mutation(source: &str, point: &MutationPoint) -> String {
    let before = &source[..point.start_byte];
    let after = &source[point.end_byte..];
    format!("{}{}{}", before, point.replacement, after)
}
```

`src/runner.rs`:
```rust
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
```

`src/report.rs`:
```rust
pub fn print_report(_killed: usize, _survived: usize) {
    println!("Report placeholder");
}
```

**Step 4: Verify compile**

Run: `cargo check`
Expected: Compiles without errors.

**Step 5: Run unit tests**

Run: `cargo test`
Expected: `test_parse_simple_ruby` passes.

**Step 6: Commit**

```bash
git add Cargo.toml src/
git commit -m "feat: scaffold ferox project with tree-sitter dependencies"
```

---

## Task 2: Find `==` and `!=` nodes in the CST

**Objective:** Walk the tree-sitter CST, find all `==` and `!=` operator nodes, extract byte ranges.

**Files:**
- Modify: `src/parser.rs`

**Step 1: Add mutation-point finding**

Replace `src/parser.rs` with:

```rust
use tree_sitter::{Node, Tree};
use crate::mutator::MutationPoint;

pub fn parse_source(source: &str) -> anyhow::Result<Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_ruby::LANGUAGE.into())?;
    let tree = parser.parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("failed to parse Ruby source"))?;
    Ok(tree)
}

/// Find all `==` and `!=` binary operator nodes in the tree,
/// returning their byte ranges for mutation.
pub fn find_eq_mutations(tree: &Tree, source: &str, file: &str) -> Vec<MutationPoint> {
    let mut points = Vec::new();
    walk_node(tree.root_node(), source, file, &mut points);
    points
}

fn walk_node(node: Node, source: &str, file: &str, points: &mut Vec<MutationPoint>) {
    // tree-sitter-ruby represents `a == b` as a `binary` node with operator child "=="
    // and `a != b` as a `binary` node with operator child "!="
    if node.kind() == "binary" {
        // Look for the operator child
        for i in 0..node.child_count() {
            let child = node.child(i).unwrap();
            if child.kind() == "==" || child.kind() == "!=" {
                let original = &source[child.start_byte()..child.end_byte()];
                let replacement = if original == "==" { "!=" } else { "==" };
                points.push(MutationPoint {
                    file: file.to_string(),
                    start_byte: child.start_byte(),
                    end_byte: child.end_byte(),
                    original: original.to_string(),
                    replacement: replacement.to_string(),
                });
            }
        }
    }

    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_node(child, source, file, points);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_ruby() {
        let tree = parse_source("x = 1 + 2").expect("should parse");
        let root = tree.root_node();
        assert!(root.child_count() > 0, "should have child nodes");
    }

    #[test]
    fn test_find_eq_operator() {
        let source = "if a == b\n  puts 'yes'\nend";
        let tree = parse_source(source).unwrap();
        let points = find_eq_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 1, "should find one == operator");
        assert_eq!(points[0].original, "==");
        assert_eq!(points[0].replacement, "!=");
    }

    #[test]
    fn test_find_neq_operator() {
        let source = "if a != b\n  puts 'no'\nend";
        let tree = parse_source(source).unwrap();
        let points = find_eq_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 1, "should find one != operator");
        assert_eq!(points[0].original, "!=");
        assert_eq!(points[0].replacement, "==");
    }

    #[test]
    fn test_find_both_operators() {
        let source = "def check(a, b)\n  a == b && a != b\nend";
        let tree = parse_source(source).unwrap();
        let points = find_eq_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 2, "should find both == and !=");
    }

    #[test]
    fn test_no_operators() {
        let source = "x = 1 + 2";
        let tree = parse_source(source).unwrap();
        let points = find_eq_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 0, "should find no mutation points");
    }
}
```

**Step 2: Run tests**

Run: `cargo test`
Expected: All 5 tests pass.

**Step 3: Commit**

```bash
git add src/parser.rs
git commit -m "feat: find ==/!= mutation points using tree-sitter CST"
```

---

## Task 3: Implement byte-range mutation application

**Objective:** Given source text and byte-range mutation point, produce the mutated source.

**Files:**
- Modify: `src/mutator.rs`

**Step 1: Add mutation logic with tests**

Replace `src/mutator.rs`:

```rust
/// A mutation target: a byte-range in a file where an operator lives.
#[derive(Debug, Clone)]
pub struct MutationPoint {
    pub file: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub original: String,
    pub replacement: String,
}

/// Replace bytes [start_byte..end_byte] in source with `replacement`.
pub fn apply_mutation(source: &str, point: &MutationPoint) -> String {
    let before = &source[..point.start_byte];
    let after = &source[point.end_byte..];
    format!("{}{}{}", before, point.replacement, after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flip_eq_to_neq() {
        let src = "if x == y\n  puts 'yes'\nend\n";
        let point = MutationPoint {
            file: "test.rb".into(),
            start_byte: 5,  // byte offset of "=="
            end_byte: 7,
            original: "==".into(),
            replacement: "!=".into(),
        };
        let result = apply_mutation(src, &point);
        assert_eq!(result, "if x != y\n  puts 'yes'\nend\n");
    }

    #[test]
    fn test_flip_neq_to_eq() {
        let src = "a != b";
        let point = MutationPoint {
            file: "test.rb".into(),
            start_byte: 2,
            end_byte: 4,
            original: "!=".into(),
            replacement: "==".into(),
        };
        assert_eq!(apply_mutation(src, &point), "a == b");
    }

    #[test]
    fn test_mutation_preserves_rest_of_file() {
        let src = "# header\ndef foo(a, b)\n  a == b\nend\n# footer\n";
        let point = MutationPoint {
            file: "test.rb".into(),
            start_byte: 28,  // "==" at byte 28
            end_byte: 30,
            original: "==".into(),
            replacement: "!=".into(),
        };
        let result = apply_mutation(src, &point);
        assert!(result.starts_with("# header\n"));
        assert!(result.ends_with("# footer\n"));
        assert!(result.contains("a != b"));
        assert!(!result.contains("a == b"));
    }

    #[test]
    fn test_utf8_multibyte_context() {
        // "→" is 3 bytes in UTF-8
        let src = "a → b == c";
        // "→" is bytes 2..5, space is 5..6, b is 6..7, space is 7..8, "==" is 8..10
        let point = MutationPoint {
            file: "test.rb".into(),
            start_byte: 8,
            end_byte: 10,
            original: "==".into(),
            replacement: "!=".into(),
        };
        assert_eq!(apply_mutation(src, &point), "a → b != c");
    }
}
```

**Step 2: Run tests**

Run: `cargo test`
Expected: All 9 tests pass.

**Step 3: Commit**

```bash
git add src/mutator.rs
git commit -m "feat: byte-range mutation application with tests"
```

---

## Task 4: Wire core mutation testing loop

**Objective:** Walk a project directory, find all `.rb` files (excluding spec/test dirs), parse each, collect mutations, apply one-by-one, run test suite for each, collect results.

**Files:**
- Create: `src/core.rs`
- Modify: `src/main.rs`

**Step 1: Create `src/core.rs`**

```rust
use crate::mutator::MutationPoint;
use crate::parser;
use crate::runner;

pub struct MutationResult {
    pub point: MutationPoint,
    pub killed: bool,
}

/// Run mutation testing on a Ruby project directory.
pub fn run_mutation_testing(project_path: &str) -> anyhow::Result<Vec<MutationResult>> {
    // Step 1: Find all Ruby source files (exclude spec/ and test/ dirs)
    let rb_files: Vec<String> = walkdir::WalkDir::new(project_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rb"))
        .filter(|e| {
            let path_str = e.path().to_string_lossy();
            !path_str.contains("/spec/") && !path_str.contains("/test/")
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

    // Step 3: Test each mutation one at a time
    let mut results = Vec::new();
    let total = all_points.len();

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

        let all_pass = runner::run_tests(project_path).unwrap_or(false);

        // Restore original
        std::fs::write(&point.file, &original)?;

        results.push(MutationResult {
            point: point.clone(),
            killed: !all_pass, // killed = tests failed (good)
        });
    }

    Ok(results)
}
```

**Step 2: Update `src/main.rs`**

```rust
mod core;
mod mutator;
mod parser;
mod report;
mod runner;

fn main() {
    println!("ferox — mutation testing for Ruby");
}
```

**Step 3: Run `cargo check`**

Run: `cargo check`
Expected: Compiles without errors.

**Step 4: Commit**

```bash
git add src/core.rs src/main.rs
git commit -m "feat: core mutation testing loop"
```

---

## Task 5: CLI with clap and formatted report

**Objective:** Accept project path via CLI arg, run mutation testing, print human-readable report.

**Files:**
- Modify: `src/main.rs`
- Modify: `src/report.rs`

**Step 1: Add CLI argument parsing in `src/main.rs`**

Replace `src/main.rs`:

```rust
mod core;
mod mutator;
mod parser;
mod report;
mod runner;

use clap::Parser;

#[derive(Parser)]
#[command(name = "ferox")]
#[command(about = "Mutation testing for Ruby", long_about = None)]
struct Cli {
    /// Path to the Ruby project
    #[arg(short, long, default_value = ".")]
    path: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let results = core::run_mutation_testing(&cli.path)?;

    let killed = results.iter().filter(|r| r.killed).count();
    let survived = results.iter().filter(|r| !r.killed).count();
    let total = results.len();

    report::print_report(killed, survived, total, &results);

    Ok(())
}
```

**Step 2: Update `src/report.rs`**

```rust
use crate::core::MutationResult;

pub fn print_report(killed: usize, survived: usize, total: usize, results: &[MutationResult]) {
    println!();
    println!("═══════════════════════════════════");
    println!("  Ferox — Mutation Testing Report  ");
    println!("═══════════════════════════════════");
    println!();

    println!("Total mutations: {}", total);
    println!("  Killed   (tests caught it):  {}", killed);
    println!("  Survived (tests missed it):  {}", survived);
    println!();

    if total > 0 {
        let score = (killed as f64 / total as f64) * 100.0;
        println!("Mutation score: {:.1}%", score);
        println!();
    }

    if survived > 0 {
        println!("--- Surviving Mutations ---");
        for r in results.iter().filter(|r| !r.killed) {
            println!(
                "  {} (bytes {}-{}): `{}` → `{}` was not caught by tests",
                r.point.file,
                r.point.start_byte,
                r.point.end_byte,
                r.point.original,
                r.point.replacement
            );
        }
    }

    if survived == 0 && total > 0 {
        println!("All mutations were killed. Excellent test coverage!");
    }

    println!();
}
```

**Step 3: Run `cargo build`**

Run: `cargo build`
Expected: Builds without errors or warnings.

**Step 4: Commit**

```bash
git add src/main.rs src/report.rs
git commit -m "feat: CLI with clap and formatted report"
```

---

## Task 6: End-to-end smoke test

**Objective:** Create a tiny Ruby project with RSpec, run ferox against it, verify it finds mutations, runs tests, and produces a correct report.

**Step 1: Create test fixture**

```bash
mkdir -p /tmp/ferox-test/{spec,lib}
```

Create `/tmp/ferox-test/lib/checker.rb`:
```ruby
class Checker
  def self.same?(a, b)
    a == b
  end

  def self.different?(a, b)
    a != b
  end
end
```

Create `/tmp/ferox-test/spec/checker_spec.rb`:
```ruby
require_relative '../lib/checker'

RSpec.describe Checker do
  describe '.same?' do
    it 'returns true when equal' do
      expect(Checker.same?(1, 1)).to be true
    end

    it 'returns false when not equal' do
      expect(Checker.same?(1, 2)).to be false
    end
  end

  describe '.different?' do
    it 'returns true when not equal' do
      expect(Checker.different?(1, 2)).to be true
    end
  end
end
```

Create `/tmp/ferox-test/Gemfile`:
```ruby
source 'https://rubygems.org'
gem 'rspec'
```

**Step 2: Verify test project works standalone**

```bash
cd /tmp/ferox-test && bundle install && bundle exec rspec
```
Expected: 3 examples, 0 failures.

**Step 3: Run ferox**

Run: `cargo run -- --path /tmp/ferox-test`
Expected:
- Finds 2 mutation points (`==` and `!=`)
- Reports progress for each mutation
- Final report shows killed/survived/mutation score

**Step 4: Verify test project is unmodified**

```bash
cd /tmp/ferox-test && bundle exec rspec
```
Expected: Still 3 examples, 0 failures (ferox restored files correctly).

**Step 5: Clean up**

```bash
rm -rf /tmp/ferox-test
```

**Step 6: Fix any issues found, commit**

```bash
git add -A
git commit -m "fix: end-to-end smoke test corrections"
```

---

## Task 7 (stretch): Add `>=` ↔ `>` and `<=` ↔ `<` operators

**Objective:** Expand mutation support to include comparison operator boundary flips.

**Files:**
- Modify: `src/parser.rs`

**Step 1: Extend `walk_node` to handle `>=`/`>`/`<=`/`<`**

In `walk_node`, add alongside the existing `==`/`!=` check:

```rust
if child.kind() == ">=" || child.kind() == ">" || child.kind() == "<=" || child.kind() == "<" {
    let original = &source[child.start_byte()..child.end_byte()];
    let replacement = match original {
        ">=" => ">",
        ">" => ">=",
        "<=" => "<",
        "<" => "<=",
        _ => unreachable!(),
    };
    points.push(MutationPoint { ... });
}
```

**Step 2: Add tests for new operators**

```rust
#[test]
fn test_find_geq_operator() { /* ... */ }
#[test]
fn test_find_leq_operator() { /* ... */ }
```

**Step 3: Run tests, commit**

```bash
cargo test
git add src/parser.rs
git commit -m "feat: add >=/> and <=/< mutation operators"
```
