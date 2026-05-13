use crate::core::MutationResult;

pub fn print_report(results: &[MutationResult]) {
    let killed = results.iter().filter(|r| r.killed()).count();
    let survived = results.iter().filter(|r| r.survived()).count();
    let errors = results.iter().filter(|r| r.errored()).count();
    let skipped = results.iter().filter(|r| r.skipped()).count();
    let total = results.len();

    println!();
    println!("═══════════════════════════════════");
    println!("  Rubigo — Mutation Testing Report  ");
    println!("═══════════════════════════════════");
    println!();

    println!("Total mutations: {}", total);
    println!("  Killed   (tests caught it):  {}", killed);
    println!("  Survived (tests missed it):  {}", survived);
    println!("  Errors   (could not test):   {}", errors);
    if skipped > 0 {
        println!("  Skipped  (from cache):       {}", skipped);
    }
    println!();

    let testable = killed + survived;
    if testable > 0 {
        let score = (killed as f64 / testable as f64) * 100.0;
        println!(
            "Mutation score: {:.1}%  ({} / {} testable mutations)",
            score, killed, testable
        );
        println!();
    }

    if survived > 0 {
        println!("--- Surviving Mutations ---");
        for r in results.iter().filter(|r| r.survived()) {
            println!(
                "  {}:{}: `{}` → `{}` was not caught by tests",
                r.point.file, r.point.line_number, r.point.original, r.point.replacement
            );
        }
        println!();
    }

    if errors > 0 {
        println!("--- Errors (could not test) ---");
        for r in results.iter().filter(|r| r.errored()) {
            println!(
                "  {}:{}: `{}` → `{}` skipped — test suite could not run",
                r.point.file, r.point.line_number, r.point.original, r.point.replacement
            );
        }
        println!();
    }

    if skipped > 0 {
        println!("--- Skipped (from cache) ---");
        for r in results.iter().filter(|r| r.skipped()) {
            println!(
                "  {}:{}: `{}` → `{}` was previously killed",
                r.point.file, r.point.line_number, r.point.original, r.point.replacement
            );
        }
        println!();
    }

    if survived == 0 && errors == 0 && total > 0 {
        if skipped > 0 {
            println!("All new mutations were killed. Excellent test coverage! ({} cached)", skipped);
        } else {
            println!("All mutations were killed. Excellent test coverage!");
        }
    }

    println!();
}

#[cfg(test)]
mod tests {
    use crate::core::MutationResult;
    use crate::core::MutationOutcome;
    use crate::mutation::MutationPoint;

    fn mp(file: &str, line: usize, orig: &str, repl: &str) -> MutationPoint {
        MutationPoint {
            file: file.to_string(),
            line_number: line,
            node_id: 0,
            original: orig.to_string(),
            replacement: repl.to_string(),
            operator_name: "flip_equality".to_string(),
        }
    }

    fn mr(file: &str, line: usize, orig: &str, repl: &str, outcome: MutationOutcome) -> MutationResult {
        MutationResult {
            point: mp(file, line, orig, repl),
            outcome,
        }
    }

    #[test]
    fn test_empty() {
        super::print_report(&[]);
    }

    #[test]
    fn test_all_killed() {
        let results = vec![
            mr("a.rb", 3, "==", "!=", MutationOutcome::Killed),
            mr("a.rb", 7, "!=", "==", MutationOutcome::Killed),
        ];
        super::print_report(&results);
    }

    #[test]
    fn test_half_killed() {
        let results = vec![
            mr("a.rb", 1, "==", "!=", MutationOutcome::Killed),
            mr("b.rb", 2, "!=", "==", MutationOutcome::Survived),
        ];
        super::print_report(&results);
    }

    #[test]
    fn test_with_errors() {
        let results = vec![
            mr("a.rb", 1, "==", "!=", MutationOutcome::Killed),
            mr("b.rb", 1, "==", "!=", MutationOutcome::Killed),
            mr("c.rb", 1, "!=", "==", MutationOutcome::Survived),
            mr("d.rb", 1, "==", "!=", MutationOutcome::Error),
        ];
        super::print_report(&results);
    }

    #[test]
    fn test_with_skipped() {
        let results = vec![
            mr("a.rb", 1, "==", "!=", MutationOutcome::Killed),
            mr("b.rb", 2, "!=", "==", MutationOutcome::Skipped),
            mr("c.rb", 3, "==", "!=", MutationOutcome::Skipped),
        ];
        super::print_report(&results);
    }

    #[test]
    fn test_excellent_with_cache() {
        let results = vec![
            mr("a.rb", 5, "==", "!=", MutationOutcome::Killed),
            mr("b.rb", 9, "!=", "==", MutationOutcome::Skipped),
        ];
        super::print_report(&results);
    }
}
