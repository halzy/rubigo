# rubigo

Mutation testing for Ruby, written in Rust.

**Rubigo** (Latin: *rūbīgō*) means "rust" — the reddish plant blight caused by rust fungi. The Romans held an annual festival (Robigalia) to appease Robigus, the god of rust, sacrificing a dog and praying that the blight would spare their crops.

Like its namesake, rubigo infiltrates your codebase, sows corruption where it shouldn't, and shows you exactly where your tests aren't protecting you.

## Why the name?

Rust itself was named after a mold. Rubigo follows that tradition: it's a fungus (the plant disease kind), and "rub-" evokes Ruby. If your mutation score is 100%, you've successfully performed the Robigalia. If not... start sacrificing.

## What it does

- Parses Ruby source with tree-sitter (CST-walk, not regex)
- Finds 14 types of mutation targets and flips/negates/removes them
- Runs your RSpec or Minitest suite against each mutation
- Reports killed (caught), survived (missed), and errored (untestable) mutations
- Handles variable-length replacements safely (e.g., `>=` → `>` shrinks by 1 byte)

## Installation

```bash
cargo install --path .
```

Or run from source:

```bash
cargo run -- --path /path/to/ruby/project
```

### Requirements

- Rust toolchain (1.77+)
- Ruby with Bundler (for the project under test)

## Usage

```bash
rubigo --path ~/my-ruby-project
rubigo --path lib/my_file.rb                  # single file
rubigo --path . --test-cmd 'bundle exec rspec {spec_file} --tag ~db'
rubigo --path . --cache .rubigo-cache.json    # incremental runs
rubigo --path . -n 5                          # first 5 mutations only
rubigo --path . -l                            # list mutations without testing
rubigo --path . --dump-cst                    # dump CST trees for debugging
rubigo --path . -v                            # show output on survived/error
rubigo --path . -vv                           # show all output including baseline
```

### CLI Flags

| Flag | Description |
|------|-------------|
| `-p`, `--path` | Ruby project directory or single file (default: `.`) |
| `--test-cmd` | Full shell command for test suite. Use `{spec_file}` for targeted spec execution |
| `--cache` | Path to cache file for incremental runs (remembers killed mutations) |
| `-n`, `--limit` | Only test the first N mutations |
| `-l`, `--list` | List all discovered mutation points without running tests |
| `--dump-cst` | Dump concrete syntax trees with node IDs for every parsed file, then exit |
| `-v` | Verbose: show test output on surviving/error mutations |
| `-vv` | Very verbose: show all test output including baseline |

## Supported Mutation Operators

rubigo ships with **14 mutation operators**:

| Operator | Mutations |
|----------|-----------|
| Equality Flip | `==` ↔ `!=` |
| Comparison Boundary | `>=` ↔ `>`, `<=` ↔ `<` |
| Boolean Flip | `true` ↔ `false` |
| Range Flip | `..` ↔ `...` |
| Logical Operator Flip | `&&` ↔ `\|\|` |
| Negation Removal | `!expr` → `expr` |
| Arithmetic Flip | `+` ↔ `-`, `*` ↔ `/`, `%` → `/` |
| Numeric Flip | `0` ↔ `1` |
| Compound Assignment Flip | `+=` ↔ `-=`, `*=` ↔ `/=` |
| Safe Navigation → Dot | `&.` → `.` |
| And/Or Flip | `and` ↔ `or` |
| Predicate Negation | `x.empty?` → `!(x.empty?)` |
| Condition Negation | `if x` → `if !(x)` (also `unless`, `while`, `until`) |
| If/Unless Keyword Swap | `if` ↔ `unless`, `while` ↔ `until` |

Operators are context-aware: `class Foo < Bar` won't produce a false `comparison_boundary` mutation, `!x.empty?` won't double-apply predicate negation inside existing negation, and modifier forms (`do_thing if x`) are handled separately.

## Supported Test Frameworks

- **RSpec** — detected by `spec/` directory. Targeted spec files are derived automatically: `app/models/user.rb` → `spec/models/user_spec.rb`, `lib/foo/bar.rb` → `spec/lib/foo/bar_spec.rb`
- **Minitest** — detected by `test/` directory (always runs full suite via `bundle exec rake test`)

Or use `--test-cmd` for custom test commands with `{spec_file}` template substitution.

## Output Example

```
Found 23 mutation points across 4 Ruby files

Running baseline test suite...
Baseline: 1.2s per run ~ est. total: ~27.6s for 23 mutations

[1/23] lib/checker.rb:12  == -> !=  [KILLED / flip_equality]  est. remaining: ~26s
[2/23] lib/checker.rb:17  && -> ||  [SURVIVED / flip_logical]  est. remaining: ~24s
[3/23] app/models/user.rb:5  >= -> >  [KILLED / comparison_boundary]  est. remaining: ~22s
...

═══════════════════════════════════
  Rubigo — Mutation Testing Report
═══════════════════════════════════

Total mutations: 23
  Killed   (tests caught it):  20
  Survived (tests missed it):   2
  Errors   (could not test):    1

Mutation score: 90.9%  (20 / 22 testable mutations)

--- Surviving Mutations ---
  lib/checker.rb:17: `&&` → `||` was not caught by tests
  lib/helpers.rb:8: `.` → `&.` was not caught by tests
```

## Architecture

rubigo uses **CST-walk mutation** via tree-sitter, not byte-offset splicing. This means it handles operators of any length — a `>=` → `>` mutation (2 bytes → 1 byte) doesn't corrupt surrounding source. The parse tree is kept alive and walked for each mutation to regenerate correct source text.

### Key design decisions

- **Tri-state outcome model**: Pass (survived), Fail (killed), Error (could not run). No binary killed/survived that hides infrastructure failures.
- **FileGuard**: Atomic file mutation with backup. If rubigo crashes between writing a mutation and restoring the original, run `mv foo.rb.rubigo-bak foo.rb`. Post-restore verification detects Spring/Bootsnap write-back of cached mutated content.
- **Pluggable operators**: Implement `MutationOperator` trait, register in `OperatorRegistry::default_operators()`. Adding an operator is a one-liner.
- **Mutation caching**: JSON cache keyed by `file + line_number + original_operator`. Skip previously killed mutations on re-runs.
- **Ctrl-C handling**: First interrupt finishes the current mutation gracefully, second interrupt exits immediately.

## License

MIT
