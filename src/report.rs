use crate::core::MutationResult;

pub fn print_report(killed: usize, survived: usize, total: usize, results: &[MutationResult]) {
    println!();
    println!("═══════════════════════════════════");
    println!("  Rubigo — Mutation Testing Report  ");
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
