use clap::Parser;

#[derive(Parser)]
#[command(name = "rubigo")]
#[command(about = "Mutation testing for Ruby. Latin for rust — the plant blight.", long_about = None)]
struct Cli {
    /// Path to the Ruby project
    #[arg(short, long, default_value = ".")]
    path: String,

    /// Extra arguments passed to rspec (e.g., --tag ~integration --tag ~db)
    #[arg(long = "rspec-arg", value_name = "ARG")]
    rspec_args: Vec<String>,

    /// Limit to the first N mutations (useful for quick checks)
    #[arg(short = 'n', long = "limit", value_name = "N")]
    limit: Option<usize>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let results = rubigo::core::run_mutation_testing(&cli.path, &cli.rspec_args, cli.limit)?;

    let killed = results.iter().filter(|r| r.killed()).count();
    let survived = results.iter().filter(|r| r.survived()).count();
    let errors = results.iter().filter(|r| r.errored()).count();
    let total = results.len();

    rubigo::report::print_report(killed, survived, errors, total, &results);

    Ok(())
}
