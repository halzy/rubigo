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
