/// A mutation target: a byte-range in a file where an operator lives.
#[derive(Debug, Clone)]
pub struct MutationPoint {
    pub file: String,
    pub line_number: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub original: String,
    pub replacement: String,
}

/// Convert a byte offset to a 1-indexed line number by counting newlines.
pub fn byte_to_line(source: &str, byte_offset: usize) -> usize {
    source[..byte_offset].chars().filter(|&c| c == '\n').count() + 1
}

/// Replace bytes [start_byte..end_byte] in source with `replacement`.
pub fn apply_mutation(source: &str, point: &MutationPoint) -> String {
    let before = &source[..point.start_byte];
    let after = &source[point.end_byte..];
    format!("{}{}{}", before, point.replacement, after)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(file: &str, line: usize, start: usize, end: usize, orig: &str, repl: &str) -> MutationPoint {
        MutationPoint {
            file: file.to_string(),
            line_number: line,
            start_byte: start,
            end_byte: end,
            original: orig.to_string(),
            replacement: repl.to_string(),
        }
    }

    #[test]
    fn test_byte_to_line_first_line() {
        assert_eq!(byte_to_line("hello world", 3), 1);
    }

    #[test]
    fn test_byte_to_line_second_line() {
        assert_eq!(byte_to_line("hello\nworld\n", 8), 2);
    }

    #[test]
    fn test_byte_to_line_at_newline_boundary() {
        assert_eq!(byte_to_line("line1\nline2\nline3", 5), 1); // byte 5 is the \n, still line 1
        assert_eq!(byte_to_line("line1\nline2\nline3", 6), 2); // byte 6 is 'l' of line2
    }

    #[test]
    fn test_flip_eq_to_neq() {
        let src = "if x == y\n  puts 'yes'\nend\n";
        let point = pt("test.rb", 1, 5, 7, "==", "!=");
        let result = apply_mutation(src, &point);
        assert_eq!(result, "if x != y\n  puts 'yes'\nend\n");
    }

    #[test]
    fn test_flip_neq_to_eq() {
        let src = "a != b";
        let point = pt("test.rb", 1, 2, 4, "!=", "==");
        assert_eq!(apply_mutation(src, &point), "a == b");
    }

    #[test]
    fn test_mutation_preserves_rest_of_file() {
        let src = "# header\ndef foo(a, b)\n  a == b\nend\n# footer\n";
        let point = pt("test.rb", 3, 27, 29, "==", "!=");
        let result = apply_mutation(src, &point);
        assert!(result.starts_with("# header\n"));
        assert!(result.ends_with("# footer\n"));
        assert!(result.contains("a != b"));
        assert!(!result.contains("a == b"));
    }

    #[test]
    fn test_utf8_multibyte_context() {
        // "→" is 3 bytes in UTF-8
        let src = "a → b == c";
        let point = pt("test.rb", 1, 8, 10, "==", "!=");
        assert_eq!(apply_mutation(src, &point), "a → b != c");
    }
}
