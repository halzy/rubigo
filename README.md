# rubigo

Mutation testing for Ruby, written in Rust.

**Rubigo** (Latin: *rūbīgō*) means "rust" — the reddish plant blight caused by rust fungi. The Romans held an annual festival (Robigalia) to appease Robigus, the god of rust, sacrificing a dog and praying that the blight would spare their crops.

Like its namesake, rubigo infiltrates your codebase, sows corruption where it shouldn't, and shows you exactly where your tests aren't protecting you.

## Why the name?

Rust itself was named after a mold. Rubigo follows that tradition: it's a fungus (the plant disease kind), and "rub-" evokes Ruby. If your mutation score is 100%, you've successfully performed the Robigalia. If not... start sacrificing.

## What it does

- Parses Ruby source code with tree-sitter
- Finds `==` and `!=` operators and flips them (`==` → `!=`, `!=` → `==`)
- Runs your existing RSpec or Minitest suite against each mutation
- Reports which mutations were killed (caught by tests) and which survived

## Installation

```bash
cargo install --path .
```

Or run from source:

```bash
cargo run -- --path /path/to/ruby/project
```

## Usage

```bash
rubigo --path ~/my-ruby-project
```

Output:

```
Found 2 mutation points across 1 Ruby files
[1/2] Testing lib/checker.rb (== -> !=) at bytes 43-45
[2/2] Testing lib/checker.rb (!= -> ==) at bytes 89-91

═══════════════════════════════════
  Rubigo — Mutation Testing Report
═══════════════════════════════════

Total mutations: 2
  Killed   (tests caught it):  2
  Survived (tests missed it):  0

Mutation score: 100.0%

All mutations were killed. Excellent test coverage!
```

## Supported test frameworks

- **RSpec** — detected by presence of a `spec/` directory
- **Minitest** — detected by presence of a `test/` directory

## Supported mutation operators

- `==` ↔ `!=` (equality flip)

More operators coming soon.

## Requirements

- Rust toolchain (1.77+)
- Ruby with Bundler (for the project under test)

## License

MIT
