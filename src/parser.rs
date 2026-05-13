use tree_sitter::Tree;

pub fn parse_source(source: &str) -> anyhow::Result<Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_ruby::LANGUAGE.into())?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("failed to parse Ruby source"))?;
    Ok(tree)
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
}
