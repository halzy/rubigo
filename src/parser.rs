use tree_sitter::{Node, Tree};

use crate::mutation::MutationPoint;
use crate::operator::OperatorRegistry;

pub fn parse_source(source: &str) -> anyhow::Result<Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_ruby::LANGUAGE.into())?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("failed to parse Ruby source"))?;
    Ok(tree)
}

/// Find all mutable operators in the tree using the default operator registry.
pub fn find_mutations(tree: &Tree, source: &str, file: &str) -> Vec<MutationPoint> {
    let registry = OperatorRegistry::default_operators();
    let mut points = Vec::new();
    walk(node_ref(tree.root_node()), source, file, &registry, &mut points);
    points
}

fn walk(node: Node, source: &str, file: &str, reg: &OperatorRegistry, points: &mut Vec<MutationPoint>) {
    points.extend(reg.try_mutate(&node, source, file));
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            walk(child, source, file, reg, points);
        }
    }
}

// tree-sitter 0.26 requires `Node<'_>` lifetime, but our walk function
// uses owned `Node` values. This helper bridges the gap.
fn node_ref(node: Node) -> Node {
    node
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_ruby() {
        let tree = parse_source("x = 1 + 2").expect("should parse");
        let root = tree.root_node();
        assert!(root.child_count() > 0, "should have child nodes");
    }

    #[test]
    fn test_find_eq_operator() {
        let source = "if a == b\n  puts 'yes'\nend";
        let tree = parse_source(source).unwrap();
        let points = find_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].original, "==");
        assert_eq!(points[0].replacement, "!=");
        assert_eq!(points[0].line_number, 1);
    }

    #[test]
    fn test_find_neq_operator() {
        let source = "if a != b\n  puts 'no'\nend";
        let tree = parse_source(source).unwrap();
        let points = find_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].original, "!=");
        assert_eq!(points[0].replacement, "==");
    }

    #[test]
    fn test_find_both_operators() {
        let source = "def check(a, b)\n  a == b && a != b\nend";
        let tree = parse_source(source).unwrap();
        let points = find_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].line_number, 2);
        assert_eq!(points[1].line_number, 2);
    }

    #[test]
    fn test_no_operators() {
        let source = "x = 1 + 2";
        let tree = parse_source(source).unwrap();
        let points = find_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 0);
    }

    #[test]
    fn test_line_number_multiline() {
        let source = "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n";
        let tree = parse_source(source).unwrap();
        let points = find_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].line_number, 3);
    }
}
