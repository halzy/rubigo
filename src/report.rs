use crate::core::MutationResult;

pub fn print_report(killed: usize, survived: usize, errors: usize, total: usize, results: &[MutationResult]) {
    println!();
    println!("═══════════════════════════════════");
    println!("  Rubigo — Mutation Testing Report  ");
    println!("═══════════════════════════════════");
    println!();

    println!("Total mutations: {}", total);
    println!("  Killed   (tests caught it):  {}", killed);
    println!("  Survived (tests missed it):  {}", survived);
    println!("  Errors   (could not test):   {}", errors);
    println!();

    let testable = killed + survived;
    if testable > 0 {
        let score = (killed as f64 / testable as f64) * 100.0;
        println!("Mutation score: {:.1}%  ({} / {} testable mutations)", score, killed, testable);
        println!();
    }

    if survived > 0 {
        println!("--- Surviving Mutations ---");
        for r in results.iter().filter(|r| r.survived()) {
            println!(
                "  {} (bytes {}-{}): `{}` → `{}` was not caught by tests",
                r.point.file,
                r.point.start_byte,
                r.point.end_byte,
                r.point.original,
                r.point.replacement
            );
        }
        println!();
    }

    if errors > 0 {
        println!("--- Errors (could not test) ---");
        for r in results.iter().filter(|r| r.errored()) {
            println!(
                "  {} (bytes {}-{}): `{}` → `{}` skipped — test suite could not run",
                r.point.file,
                r.point.start_byte,
                r.point.end_byte,
                r.point.original,
                r.point.replacement
            );
        }
        println!();
    }

    if survived == 0 && errors == 0 && total > 0 {
        println!("All mutations were killed. Excellent test coverage!");
    }

    println!();
}
