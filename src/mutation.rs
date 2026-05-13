/// A mutation target: a node in the CST identified by tree-sitter node id.
#[derive(Debug, Clone)]
pub struct MutationPoint {
    pub file: String,
    pub line_number: usize,
    pub node_id: usize,
    pub original: String,
    pub replacement: String,
}

/// Convert a byte offset to a 1-indexed line number by counting newlines.
pub fn byte_to_line(source: &str, byte_offset: usize) -> usize {
    source[..byte_offset].chars().filter(|&c| c == '\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(byte_to_line("line1\nline2\nline3", 5), 1);
        assert_eq!(byte_to_line("line1\nline2\nline3", 6), 2);
    }
}
