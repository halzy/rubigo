use clap::Parser;

use rubigo::config::{Config, Verbosity};

#[derive(Parser)]
#[command(name = "rubigo")]
#[command(about = "Mutation testing for Ruby. Latin for rust — the plant blight.", long_about = None)]
struct Cli {
    /// Path to the Ruby project
    #[arg(short, long, default_value = ".")]
    path: String,

    /// Full shell command to run the test suite. Use {spec_file} to target the
    /// spec file for the current mutated source file (e.g., "bundle exec rspec {spec_file}").
    /// Without this flag, the test framework is auto-detected.
    #[arg(long = "test-cmd", value_name = "CMD")]
    test_cmd: Option<String>,

    /// Cache file for previously killed mutations
    #[arg(long = "cache", value_name = "FILE")]
    cache: Option<String>,

    /// Limit to the first N mutations
    #[arg(short = 'n', long = "limit", value_name = "N")]
    limit: Option<usize>,

    /// List all discovered mutation points without running tests
    #[arg(short = 'l', long = "list")]
    list: bool,

    /// Dump the CST with node ids for every source file, then exit.
    /// Useful for debugging false-positive mutations.
    #[arg(long = "dump-cst")]
    dump_cst: bool,

    /// Increase verbosity (-v: show on failure, -vv: show everything)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    verbosity: u8,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let cfg = Config {
        project_path: &cli.path,
        test_cmd: cli.test_cmd.as_deref(),
        cache_path: cli.cache.as_deref(),
        limit: cli.limit,
        list_only: cli.list,
        dump_cst: cli.dump_cst,
        verbosity: Verbosity::from_count(cli.verbosity),
    };

    let results = rubigo::core::run_mutation_testing(&cfg)?;
    rubigo::report::print_report(&results);

    Ok(())
}
