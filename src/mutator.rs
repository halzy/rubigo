/// A mutation target: a byte-range in a file where an operator lives.
#[derive(Debug, Clone)]
pub struct MutationPoint {
    pub file: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub original: String,
    pub replacement: String,
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

    #[test]
    fn test_flip_eq_to_neq() {
        let src = "if x == y\n  puts 'yes'\nend\n";
        let point = MutationPoint {
            file: "test.rb".into(),
            start_byte: 5, // byte offset of "=="
            end_byte: 7,
            original: "==".into(),
            replacement: "!=".into(),
        };
        let result = apply_mutation(src, &point);
        assert_eq!(result, "if x != y\n  puts 'yes'\nend\n");
    }

    #[test]
    fn test_flip_neq_to_eq() {
        let src = "a != b";
        let point = MutationPoint {
            file: "test.rb".into(),
            start_byte: 2,
            end_byte: 4,
            original: "!=".into(),
            replacement: "==".into(),
        };
        assert_eq!(apply_mutation(src, &point), "a == b");
    }

    #[test]
    fn test_mutation_preserves_rest_of_file() {
        let src = "# header\ndef foo(a, b)\n  a == b\nend\n# footer\n";
        let point = MutationPoint {
            file: "test.rb".into(),
            start_byte: 27,
            end_byte: 29,
            original: "==".into(),
            replacement: "!=".into(),
        };
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
        let point = MutationPoint {
            file: "test.rb".into(),
            start_byte: 8,
            end_byte: 10,
            original: "==".into(),
            replacement: "!=".into(),
        };
        assert_eq!(apply_mutation(src, &point), "a → b != c");
    }
}
