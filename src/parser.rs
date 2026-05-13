use tree_sitter::{Node, Tree};

use crate::mutator::{self, MutationPoint};

pub fn parse_source(source: &str) -> anyhow::Result<Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_ruby::LANGUAGE.into())?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("failed to parse Ruby source"))?;
    Ok(tree)
}

/// Find all `==` and `!=` operator nodes in the tree,
/// returning their byte ranges and line numbers for mutation.
pub fn find_eq_mutations(tree: &Tree, source: &str, file: &str) -> Vec<MutationPoint> {
    let mut points = Vec::new();
    walk_node(tree.root_node(), source, file, &mut points);
    points
}

fn walk_node(node: Node, source: &str, file: &str, points: &mut Vec<MutationPoint>) {
    // In tree-sitter-ruby, `a == b` is a `binary` node with "==" operator child,
    // and `a != b` is a `binary` node with "!=" operator child.
    if node.kind() == "binary" {
        for i in 0..node.child_count() {
            let child = node.child(i as u32).unwrap();
            if child.kind() == "==" || child.kind() == "!=" {
                let start_byte = child.start_byte();
                let original = &source[start_byte..child.end_byte()];
                let replacement = if original == "==" { "!=" } else { "==" };
                let line_number = mutator::byte_to_line(source, start_byte);
                points.push(MutationPoint {
                    file: file.to_string(),
                    line_number,
                    start_byte,
                    end_byte: child.end_byte(),
                    original: original.to_string(),
                    replacement: replacement.to_string(),
                });
            }
        }
    }

    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            walk_node(child, source, file, points);
        }
    }
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
        let points = find_eq_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 1, "should find one == operator");
        assert_eq!(points[0].original, "==");
        assert_eq!(points[0].replacement, "!=");
        assert_eq!(points[0].line_number, 1, "== is on line 1");
    }

    #[test]
    fn test_find_neq_operator() {
        let source = "if a != b\n  puts 'no'\nend";
        let tree = parse_source(source).unwrap();
        let points = find_eq_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 1, "should find one != operator");
        assert_eq!(points[0].original, "!=");
        assert_eq!(points[0].replacement, "==");
    }

    #[test]
    fn test_find_both_operators() {
        let source = "def check(a, b)\n  a == b && a != b\nend";
        let tree = parse_source(source).unwrap();
        let points = find_eq_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 2, "should find both == and !=");
        assert_eq!(points[0].line_number, 2);
        assert_eq!(points[1].line_number, 2);
    }

    #[test]
    fn test_no_operators() {
        let source = "x = 1 + 2";
        let tree = parse_source(source).unwrap();
        let points = find_eq_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 0, "should find no mutation points");
    }

    #[test]
    fn test_line_number_multiline() {
        let source = "class Foo\n  def bar(a, b)\n    a == b\n  end\nend\n";
        let tree = parse_source(source).unwrap();
        let points = find_eq_mutations(&tree, source, "test.rb");
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].line_number, 3);
    }
}
