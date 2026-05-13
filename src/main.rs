use clap::Parser;

#[derive(Parser)]
#[command(name = "rubigo")]
#[command(about = "Mutation testing for Ruby. Latin for rust — the plant blight.", long_about = None)]
struct Cli {
    /// Path to the Ruby project
    #[arg(short, long, default_value = ".")]
    path: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let results = rubigo::core::run_mutation_testing(&cli.path)?;

    let killed = results.iter().filter(|r| r.killed).count();
    let survived = results.iter().filter(|r| !r.killed).count();
    let total = results.len();

    rubigo::report::print_report(killed, survived, total, &results);

    Ok(())
}
