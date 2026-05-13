use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io;
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
    pub fn from_point(point: &crate::mutation::MutationPoint) -> Self {
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
pub fn save_cache(path: &Path, killed: &[MutationId]) -> io::Result<()> {
    let mut existing = load_cache(path);
    for id in killed {
        existing.insert(id.clone());
    }
    let contents = serde_json::to_string_pretty(&Vec::from_iter(existing.into_iter()))
        .unwrap_or_else(|_| "[]".into());
    std::fs::write(path, contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(file: &str, line: usize, original: &str) -> MutationId {
        MutationId {
            file: file.to_string(),
            line_number: line,
            original: original.to_string(),
        }
    }

    #[test]
    fn test_load_cache_missing_file_returns_empty() {
        let result = load_cache(Path::new("/tmp/rubigo-nonexistent-cache.json"));
        assert!(result.is_empty());
    }

    #[test]
    fn test_load_cache_corrupted_json_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json").unwrap();
        let result = load_cache(&path);
        assert!(result.is_empty());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");

        let killed = vec![
            id("a.rb", 3, "=="),
            id("b.rb", 7, "!="),
        ];
        save_cache(&path, &killed).unwrap();

        let loaded = load_cache(&path);
        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains(&id("a.rb", 3, "==")));
        assert!(loaded.contains(&id("b.rb", 7, "!=")));
    }

    #[test]
    fn test_save_cache_appends_to_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");

        // First save
        save_cache(&path, &[id("a.rb", 1, "==")]).unwrap();
        // Second save should append
        save_cache(&path, &[id("b.rb", 2, "!=")]).unwrap();

        let loaded = load_cache(&path);
        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains(&id("a.rb", 1, "==")));
        assert!(loaded.contains(&id("b.rb", 2, "!=")));
    }

    #[test]
    fn test_save_cache_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");

        // Save the same id twice
        save_cache(&path, &[id("a.rb", 1, "==")]).unwrap();
        save_cache(&path, &[id("a.rb", 1, "==")]).unwrap();

        let loaded = load_cache(&path);
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn test_mutation_id_equality() {
        let a = id("a.rb", 1, "==");
        let b = id("a.rb", 1, "==");
        let c = id("a.rb", 2, "==");
        let d = id("a.rb", 1, "!=");

        assert_eq!(a, b);  // same file + line + operator
        assert_ne!(a, c);  // different line
        assert_ne!(a, d);  // different operator
    }
}
