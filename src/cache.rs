use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// A unique identity for a mutation: file + line + original operator.
/// Used for caching killed mutations across runs.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutationId {
    pub file: String,
    pub line_number: usize,
    pub original: String,
}

impl MutationId {
    pub fn from_point(point: &crate::mutator::MutationPoint) -> Self {
        MutationId {
            file: point.file.clone(),
            line_number: point.line_number,
            original: point.original.clone(),
        }
    }
}

/// Load previously killed mutation IDs from a JSON cache file.
pub fn load_cache(path: &Path) -> HashSet<MutationId> {
    match std::fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => HashSet::new(),
    }
}

/// Save newly killed mutation IDs to the cache file.
/// Appends to existing entries (loads, merges, writes back).
pub fn save_cache(path: &Path, killed: &[MutationId]) {
    let mut existing = load_cache(path);
    for id in killed {
        existing.insert(id.clone());
    }
    let contents = serde_json::to_string_pretty(&Vec::from_iter(existing.into_iter()))
        .unwrap_or_else(|_| "[]".into());
    let _ = std::fs::write(path, contents);
}
