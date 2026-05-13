use clap::Parser;

#[derive(Parser)]
#[command(name = "rubigo")]
#[command(about = "Mutation testing for Ruby. Latin for rust — the plant blight.", long_about = None)]
struct Cli {
    /// Path to the Ruby project
    #[arg(short, long, default_value = ".")]
    path: String,

    /// Full shell command to run the test suite. Supports env vars, pipes, etc.
    /// If not provided, auto-detects RSpec or Minitest.
    /// Example: DATABASE_URL=... bundle exec rspec --tag ~db
    #[arg(long = "test-cmd", value_name = "CMD")]
    test_cmd: Option<String>,

    /// Cache file for previously killed mutations (skip them on re-runs)
    #[arg(long = "cache", value_name = "FILE")]
    cache: Option<String>,

    /// Limit to the first N mutations (useful for quick checks)
    #[arg(short = 'n', long = "limit", value_name = "N")]
    limit: Option<usize>,

    /// Increase verbosity (-v: show output on SURVIVED/ERROR, -vv: always show)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    verbosity: u8,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let results = rubigo::core::run_mutation_testing(
        &cli.path,
        cli.test_cmd.as_deref(),
        cli.cache.as_deref(),
        cli.limit,
        cli.verbosity,
    )?;

    let killed = results.iter().filter(|r| r.killed()).count();
    let survived = results.iter().filter(|r| r.survived()).count();
    let errors = results.iter().filter(|r| r.errored()).count();
    let skipped = results.iter().filter(|r| r.skipped()).count();
    let total = results.len();

    rubigo::report::print_report(killed, survived, errors, skipped, total, &results);

    Ok(())
}
