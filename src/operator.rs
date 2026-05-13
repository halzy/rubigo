use std::fmt::Debug;
use tree_sitter::Node;

use crate::mutation::{byte_to_line, MutationPoint};

/// A single mutation operator. Pluggable — new operators added
/// by implementing this trait and registering them.
pub trait MutationOperator: Debug + Send + Sync {
    /// Human-readable name for reports.
    fn name(&self) -> &str;

    /// Returns true if this node should be mutated by this operator.
    /// The `parent_kind` is the kind string of the parent CST node, or None at root.
    fn can_mutate(&self, node: &Node, source: &str, parent_kind: Option<&str>) -> bool;

    /// Produce a mutation point from this node, or None.
    fn mutate(&self, node: &Node, source: &str, file: &str) -> Option<MutationPoint>;
}

/// Holds the set of registered mutation operators.
/// The parser iterates these against every CST node.
pub struct OperatorRegistry {
    operators: Vec<Box<dyn MutationOperator>>,
}

impl OperatorRegistry {
    /// Create a registry with the default set of operators.
    pub fn default_operators() -> Self {
        let mut reg = Self::new();
        reg.register(Box::new(EqualityFlip));
        reg.register(Box::new(ComparisonBoundary));
        reg
    }

    pub fn new() -> Self {
        OperatorRegistry {
            operators: Vec::new(),
        }
    }

    pub fn register(&mut self, op: Box<dyn MutationOperator>) {
        self.operators.push(op);
    }

    /// Run all operators against a node. Returns Vec because multiple
    /// operators might match the same node (e.g., both == and != on same node).
    pub fn try_mutate(
        &self,
        node: &Node,
        source: &str,
        file: &str,
        parent_kind: Option<&str>,
    ) -> Vec<MutationPoint> {
        self.operators
            .iter()
            .filter_map(|op| {
                if op.can_mutate(node, source, parent_kind) {
                    op.mutate(node, source, file)
                } else {
                    None
                }
            })
            .collect()
    }
}

// ── Equality Flip: == ↔ != ──────────────────────────────

#[derive(Debug)]
pub struct EqualityFlip;

impl MutationOperator for EqualityFlip {
    fn name(&self) -> &str {
        "flip_equality"
    }

    fn can_mutate(&self, node: &Node, _source: &str, _parent_kind: Option<&str>) -> bool {
        node.kind() == "==" || node.kind() == "!="
    }

    fn mutate(&self, node: &Node, source: &str, file: &str) -> Option<MutationPoint> {
        let original = &source[node.start_byte()..node.end_byte()];
        let replacement = if original == "==" { "!=" } else { "==" };
        Some(MutationPoint {
            file: file.to_string(),
            line_number: byte_to_line(source, node.start_byte()),
            node_id: node.id(),
            original: original.to_string(),
            replacement: replacement.to_string(),
            operator_name: self.name().to_string(),
        })
    }
}

// ── Comparison Boundary: >= ↔ >  and  <= ↔ < ────────────

#[derive(Debug)]
pub struct ComparisonBoundary;

impl MutationOperator for ComparisonBoundary {
    fn name(&self) -> &str {
        "comparison_boundary"
    }

    fn can_mutate(&self, node: &Node, _source: &str, parent_kind: Option<&str>) -> bool {
        if !matches!(node.kind(), ">=" | ">" | "<=" | "<") {
            return false;
        }
        // Don't mutate < / > used in class inheritance (class Foo < Bar)
        // or singleton class (class << self). Those are structural, not comparisons.
        if let Some(pk) = parent_kind {
            if pk == "superclass" || pk == "singleton_class" {
                return false;
            }
        }
        true
    }

    fn mutate(&self, node: &Node, source: &str, file: &str) -> Option<MutationPoint> {
        let original = &source[node.start_byte()..node.end_byte()];
        let replacement = match original {
            ">=" => ">",
            ">" => ">=",
            "<=" => "<",
            "<" => "<=",
            _ => return None,
        };
        Some(MutationPoint {
            file: file.to_string(),
            line_number: byte_to_line(source, node.start_byte()),
            node_id: node.id(),
            original: original.to_string(),
            replacement: replacement.to_string(),
            operator_name: self.name().to_string(),
        })
    }
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn parse_and_find(source: &str, operator: &dyn MutationOperator) -> Vec<MutationPoint> {
        let tree = parser::parse_source(source).unwrap();
        let mut points = Vec::new();
        walk_test(tree.root_node(), source, "test.rb", operator, None, &mut points);
        points
    }

    fn walk_test(
        node: Node,
        source: &str,
        file: &str,
        op: &dyn MutationOperator,
        parent_kind: Option<&str>,
        points: &mut Vec<MutationPoint>,
    ) {
        if op.can_mutate(&node, source, parent_kind) {
            if let Some(pt) = op.mutate(&node, source, file) {
                points.push(pt);
            }
        }
        let kind_str = node.kind().to_string();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                walk_test(child, source, file, op, Some(&kind_str), points);
            }
        }
    }

    #[test]
    fn test_registry_finds_equality_flips() {
        let reg = OperatorRegistry::default_operators();
        let source = "a == b";
        let tree = parser::parse_source(source).unwrap();
        let points = find_all(&reg, &tree, source, "test.rb");
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].original, "==");
    }

    #[test]
    fn test_registry_finds_comparisons() {
        let reg = OperatorRegistry::default_operators();
        let source = "if a >= b && c < d\nend";
        let tree = parser::parse_source(source).unwrap();
        let points = find_all(&reg, &tree, source, "test.rb");
        assert_eq!(points.len(), 2);
        assert!(points.iter().any(|p| p.original == ">="));
        assert!(points.iter().any(|p| p.original == "<"));
    }

    #[test]
    fn test_comparison_flip_geq_to_gt() {
        let pts = parse_and_find("x >= 5", &ComparisonBoundary);
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].original, ">=");
        assert_eq!(pts[0].replacement, ">");
    }

    #[test]
    fn test_comparison_flip_gt_to_geq() {
        let pts = parse_and_find("x > 5", &ComparisonBoundary);
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].original, ">");
        assert_eq!(pts[0].replacement, ">=");
    }

    #[test]
    fn test_comparison_flip_leq_to_lt() {
        let pts = parse_and_find("x <= 5", &ComparisonBoundary);
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].original, "<=");
        assert_eq!(pts[0].replacement, "<");
    }

    #[test]
    fn test_comparison_flip_lt_to_leq() {
        let pts = parse_and_find("x < 5", &ComparisonBoundary);
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].original, "<");
        assert_eq!(pts[0].replacement, "<=");
    }

    #[test]
    fn test_class_inheritance_not_mutated() {
        let source = "class Foo < Bar\nend\n";
        let pts = parse_and_find(source, &ComparisonBoundary);
        assert_eq!(pts.len(), 0, "class Foo < Bar should not produce mutations");
    }

    #[test]
    fn test_singleton_class_not_mutated() {
        let source = "class << self\n  def foo\n  end\nend\n";
        let pts = parse_and_find(source, &ComparisonBoundary);
        assert_eq!(pts.len(), 0, "class << self should not produce mutations");
    }

    #[test]
    fn test_class_inheritance_with_comparison_elsewhere() {
        // The < on line 1 is inheritance, but the <= on line 3 is a real comparison
        let source = "class Foo < Bar\n  def check(x)\n    x <= 10\n  end\nend\n";
        let pts = parse_and_find(source, &ComparisonBoundary);
        assert_eq!(pts.len(), 1, "only the <= comparison should be mutated");
        assert_eq!(pts[0].original, "<=");
        assert_eq!(pts[0].replacement, "<");
    }

    #[test]
    fn test_registry_full_excludes_inheritance() {
        let reg = OperatorRegistry::default_operators();
        let source = "class Foo < Bar\n  def check(x)\n    x <= 10\n  end\nend\n";
        let tree = parser::parse_source(source).unwrap();
        let points = find_all(&reg, &tree, source, "test.rb");
        assert_eq!(points.len(), 1, "registry should exclude inheritance <");
        assert_eq!(points[0].original, "<=");
    }

    fn find_all(
        reg: &OperatorRegistry,
        tree: &tree_sitter::Tree,
        source: &str,
        file: &str,
    ) -> Vec<MutationPoint> {
        let mut points = Vec::new();
        walk_all(reg, tree.root_node(), source, file, None, &mut points);
        points
    }

    fn walk_all(
        reg: &OperatorRegistry,
        node: Node,
        source: &str,
        file: &str,
        parent_kind: Option<&str>,
        points: &mut Vec<MutationPoint>,
    ) {
        points.extend(reg.try_mutate(&node, source, file, parent_kind));
        let kind_str = node.kind().to_string();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                walk_all(reg, child, source, file, Some(&kind_str), points);
            }
        }
    }
}
