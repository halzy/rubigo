use tree_sitter::{Node, Tree};

/// Walk the CST and emit reconstructed source text.
/// For the node matching `target_id`, emits `replacement` instead of
/// the original text. All other nodes emit their original source.
///
/// This handles operators of any length — the emitted text just grows
/// or shrinks at the target position; surrounding gaps are always
/// taken from the original parse.
pub fn emit_tree(tree: &Tree, source: &str, target_id: usize, replacement: &str) -> String {
    emit_node(&tree.root_node(), source, target_id, replacement)
}

fn emit_node(node: &Node, source: &str, target_id: usize, replacement: &str) -> String {
    // Hit the target — emit replacement, skip children
    if node.id() == target_id {
        return replacement.to_string();
    }

    // Leaf node — emit original source verbatim
    if node.child_count() == 0 {
        return source[node.start_byte()..node.end_byte()].to_string();
    }

    // Internal node — walk children, filling gaps from original source
    let mut result = String::new();
    let mut last_end = node.start_byte();

    for i in 0..node.child_count() {
        let child = node.child(i as u32).unwrap();
        result.push_str(&source[last_end..child.start_byte()]);
        result.push_str(&emit_node(&child, source, target_id, replacement));
        last_end = child.end_byte();
    }

    result.push_str(&source[last_end..node.end_byte()]);
    result
}

#[cfg(test)]
mod tests {
    use crate::parser;

    #[test]
    fn test_emit_no_substitution() {
        let source = "x = 1 + 2";
        let tree = parser::parse_source(source).unwrap();
        let root_id = tree.root_node().id();
        // target the root itself — emits the replacement for the whole file
        let result = super::emit_tree(&tree, source, root_id, "REPLACED");
        assert_eq!(result, "REPLACED");
    }

    #[test]
    fn test_emit_same_length_swap() {
        let source = "a == b";
        let tree = parser::parse_source(source).unwrap();
        let root = tree.root_node();
        // Find the "==" operator node
        let eq_id = find_child_by_kind(&root, "==").unwrap();
        let result = super::emit_tree(&tree, source, eq_id, "!=");
        assert_eq!(result, "a != b");
    }

    #[test]
    fn test_emit_longer_replacement() {
        let source = "a == b";
        let tree = parser::parse_source(source).unwrap();
        let root = tree.root_node();
        let eq_id = find_child_by_kind(&root, "==").unwrap();
        // Replace 2-byte "==" with 4-byte "!=="  (not valid Ruby, just testing)
        let result = super::emit_tree(&tree, source, eq_id, "!==");
        assert_eq!(result, "a !== b");
    }

    #[test]
    fn test_emit_shorter_replacement() {
        let source = "a != b";
        let tree = parser::parse_source(source).unwrap();
        let root = tree.root_node();
        let neq_id = find_child_by_kind(&root, "!=").unwrap();
        // Replace 2-byte "!=" with 1-byte "=" (not valid, just testing)
        let result = super::emit_tree(&tree, source, neq_id, "=");
        assert_eq!(result, "a = b");
    }

    #[test]
    fn test_emit_multiline_preserved() {
        let source = "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n";
        let tree = parser::parse_source(source).unwrap();
        let root = tree.root_node();
        let eq_id = find_child_by_kind(&root, "==").unwrap();
        let result = super::emit_tree(&tree, source, eq_id, "!=");
        assert!(result.contains("class Foo"));
        assert!(result.contains("def bar"));
        assert!(result.contains("a != b"));
        assert!(!result.contains("a == b"));
    }

    #[test]
    fn test_emit_utf8_multibyte_preserved() {
        let source = "a → b == c";
        let tree = parser::parse_source(source).unwrap();
        let root = tree.root_node();
        let eq_id = find_child_by_kind(&root, "==").unwrap();
        let result = super::emit_tree(&tree, source, eq_id, "!==");
        assert_eq!(result, "a → b !== c");
    }

    /// Recursively search for the first node matching `kind`.
    fn find_child_by_kind(node: &tree_sitter::Node, kind: &str) -> Option<usize> {
        if node.kind() == kind {
            return Some(node.id());
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                if let Some(id) = find_child_by_kind(&child, kind) {
                    return Some(id);
                }
            }
        }
        None
    }
}
