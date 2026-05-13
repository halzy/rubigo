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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let results = rubigo::core::run_mutation_testing(&cli.path, &cli.rspec_args)?;

    let killed = results.iter().filter(|r| r.killed()).count();
    let survived = results.iter().filter(|r| r.survived()).count();
    let errors = results.iter().filter(|r| r.errored()).count();
    let total = results.len();

    rubigo::report::print_report(killed, survived, errors, total, &results);

    Ok(())
}
