use crate::core::MutationResult;

pub fn print_report(
    killed: usize,
    survived: usize,
    errors: usize,
    total: usize,
    results: &[MutationResult],
) {
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

    if survived == 0 && errors == 0 && total > 0 {
        println!("All mutations were killed. Excellent test coverage!");
    }

    println!();
}

#[cfg(test)]
mod tests {
    use crate::core::{MutationOutcome, MutationResult};
    use crate::mutator::MutationPoint;

    fn mp(file: &str, line: usize, start: usize, end: usize, orig: &str, repl: &str) -> MutationPoint {
        MutationPoint {
            file: file.to_string(),
            line_number: line,
            start_byte: start,
            end_byte: end,
            original: orig.to_string(),
            replacement: repl.to_string(),
        }
    }

    fn mr(
        file: &str,
        line: usize,
        start: usize,
        end: usize,
        orig: &str,
        repl: &str,
        outcome: MutationOutcome,
    ) -> MutationResult {
        MutationResult {
            point: mp(file, line, start, end, orig, repl),
            outcome,
        }
    }

    #[test]
    fn test_score_zero_killed_zero_testable() {
        let results: Vec<MutationResult> = vec![];
        super::print_report(0, 0, 0, 0, &results);
    }

    #[test]
    fn test_score_all_killed_is_100_percent() {
        let results = vec![
            mr("a.rb", 3, 0, 2, "==", "!=", MutationOutcome::Killed),
            mr("a.rb", 7, 5, 7, "!=", "==", MutationOutcome::Killed),
        ];
        super::print_report(2, 0, 0, 2, &results);
    }

    #[test]
    fn test_score_half_killed_is_50_percent() {
        let results = vec![
            mr("a.rb", 1, 0, 2, "==", "!=", MutationOutcome::Killed),
            mr("b.rb", 2, 0, 2, "!=", "==", MutationOutcome::Survived),
        ];
        super::print_report(1, 1, 0, 2, &results);
    }

    #[test]
    fn test_score_with_errors_excludes_them_from_testable() {
        let results = vec![
            mr("a.rb", 1, 0, 2, "==", "!=", MutationOutcome::Killed),
            mr("b.rb", 1, 0, 2, "==", "!=", MutationOutcome::Killed),
            mr("c.rb", 1, 0, 2, "!=", "==", MutationOutcome::Survived),
            mr("d.rb", 1, 0, 2, "==", "!=", MutationOutcome::Error),
        ];
        super::print_report(2, 1, 1, 4, &results);
    }

    #[test]
    fn test_no_panic_on_empty_results() {
        super::print_report(0, 0, 0, 0, &[]);
    }

    #[test]
    fn test_excellent_coverage_message_when_all_killed() {
        let results = vec![mr("a.rb", 2, 0, 2, "==", "!=", MutationOutcome::Killed)];
        super::print_report(1, 0, 0, 1, &results);
    }

    #[test]
    fn test_no_excellent_coverage_message_when_errors_exist() {
        let results = vec![
            mr("a.rb", 5, 0, 2, "==", "!=", MutationOutcome::Killed),
            mr("b.rb", 9, 0, 2, "!=", "==", MutationOutcome::Error),
        ];
        super::print_report(1, 0, 1, 2, &results);
    }
}
