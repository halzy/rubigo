use std::fmt::Debug;
use tree_sitter::Node;

use crate::mutation::{MutationPoint, byte_to_line};

/// A single mutation operator. Pluggable — new operators added
/// by implementing this trait and registering them.
pub trait MutationOperator: Debug + Send + Sync {
    /// Human-readable name for reports.
    fn name(&self) -> &str;

    /// Returns true if this node should be mutated by this operator.
    fn can_mutate(&self, node: &Node, source: &str) -> bool;

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
    pub fn try_mutate(&self, node: &Node, source: &str, file: &str) -> Vec<MutationPoint> {
        self.operators
            .iter()
            .filter_map(|op| {
                if op.can_mutate(node, source) {
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

    fn can_mutate(&self, node: &Node, _source: &str) -> bool {
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

    fn can_mutate(&self, node: &Node, _source: &str) -> bool {
        matches!(node.kind(), ">=" | ">" | "<=" | "<")
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
        walk_test(tree.root_node(), source, "test.rb", operator, &mut points);
        points
    }

    fn walk_test(node: Node, source: &str, file: &str, op: &dyn MutationOperator, points: &mut Vec<MutationPoint>) {
        if op.can_mutate(&node, source) {
            if let Some(pt) = op.mutate(&node, source, file) {
                points.push(pt);
            }
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                walk_test(child, source, file, op, points);
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

    fn find_all(reg: &OperatorRegistry, tree: &tree_sitter::Tree, source: &str, file: &str) -> Vec<MutationPoint> {
        let mut points = Vec::new();
        walk_all(reg, tree.root_node(), source, file, &mut points);
        points
    }

    fn walk_all(reg: &OperatorRegistry, node: Node, source: &str, file: &str, points: &mut Vec<MutationPoint>) {
        points.extend(reg.try_mutate(&node, source, file));
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                walk_all(reg, child, source, file, points);
            }
        }
    }
}
