use std::io;
use std::path::{Path, PathBuf};

/// Atomically overwrite a file with new content, keeping a backup.
///
/// On construction:
///   1. Writes `content` to a tempfile (`path.rubigo-tmp`)
///   2. Renames original → backup (`path.rubigo-bak`)
///   3. Renames tempfile → original
///
/// On drop (or explicit `restore`):
///   Renames backup → original, deletes backup.
///
/// If the process crashes at any point, the backup file remains
/// and the user can recover with: `mv foo.rb.rubigo-bak foo.rb`
pub struct FileGuard {
    path: PathBuf,
    backup: Option<PathBuf>,
}

impl FileGuard {
    /// Atomically replace `path` contents with `content`.
    pub fn overwrite(path: &Path, content: &str) -> io::Result<Self> {
        let bak = path.with_extension(match path.extension() {
            Some(ext) => format!("{}.rubigo-bak", ext.to_string_lossy()),
            None => "rubigo-bak".to_string(),
        });
        let tmp = path.with_extension(match path.extension() {
            Some(ext) => format!("{}.rubigo-tmp", ext.to_string_lossy()),
            None => "rubigo-tmp".to_string(),
        });

        // 1. Write new content to tempfile and fsync to disk
        {
            let f = std::fs::File::create(&tmp)?;
            use std::io::Write;
            let mut f = std::io::BufWriter::new(f);
            f.write_all(content.as_bytes())?;
            f.flush()?;
            f.into_inner().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "BufWriter flush failed")
            })?.sync_all()?;
        }

        // 2. Rename original → backup (atomic on same filesystem)
        std::fs::rename(path, &bak)?;

        // 3. Rename tempfile → original (atomic on same filesystem)
        std::fs::rename(&tmp, path)?;

        Ok(FileGuard {
            path: path.to_path_buf(),
            backup: Some(bak),
        })
    }

    /// Restore the original file and clean up the backup.
    pub fn restore(&mut self) -> io::Result<()> {
        if let Some(ref bak) = self.backup.take() {
            // Clean up any stale tempfile from a prior crash
            let tmp = self.path.with_extension(match self.path.extension() {
                Some(ext) => format!("{}.rubigo-tmp", ext.to_string_lossy()),
                None => "rubigo-tmp".to_string(),
            });
            let _ = std::fs::remove_file(&tmp);

            if !bak.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "Backup file missing — {} may be left in mutated state. \
                         Recover from git: `git checkout -- {}`",
                        bak.display(),
                        self.path.display(),
                    ),
                ));
            }

            std::fs::rename(bak, &self.path)?;
        }
        Ok(())
    }

    /// Consume the guard without restoring (caller has already handled it).
    pub fn disarm(mut self) {
        self.backup = None;
    }
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overwrite_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rb");
        std::fs::write(&path, "original content").unwrap();

        {
            let mut guard = FileGuard::overwrite(&path, "mutated content").unwrap();
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                "mutated content",
                "file should contain mutated content"
            );
            let bak = path.with_extension("rb.rubigo-bak");
            assert!(bak.exists(), "backup (original) should exist while guard is active");
            guard.restore().unwrap();
        }

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "original content",
            "file should be restored"
        );
        let bak = path.with_extension("rb.rubigo-bak");
        assert!(!bak.exists(), "backup should be cleaned up");
    }

    #[test]
    fn test_guard_restores_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rb");
        std::fs::write(&path, "original").unwrap();

        {
            let _guard = FileGuard::overwrite(&path, "mutated").unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "mutated");
            // Drop without explicit restore
        }

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "original",
            "drop should restore original"
        );
    }

    #[test]
    fn test_disarm_prevents_restore() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rb");
        std::fs::write(&path, "original").unwrap();

        {
            let guard = FileGuard::overwrite(&path, "changed").unwrap();
            guard.disarm();
            // Drop happens here — but restore is skipped
        }

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "changed",
            "disarm should prevent restore on drop"
        );

        // Clean up
        let bak = path.with_extension("rb.rubigo-bak");
        let _ = std::fs::remove_file(&bak);
    }

    #[test]
    fn test_backup_exists_after_crash_scenario() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rb");
        let bak = path.with_extension("rb.rubigo-bak");
        std::fs::write(&path, "original").unwrap();

        // Simulate: create guard, then manually corrupt state
        // (backup exists but guard was lost — crash simulation)
        let guard = FileGuard::overwrite(&path, "mutated").unwrap();
        std::mem::forget(guard); // simulate crash — drop never runs

        assert!(
            bak.exists(),
            "backup should exist after simulated crash"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "mutated",
            "file contains mutated content from partial run"
        );

        // Recovery: user renames backup
        std::fs::rename(&bak, &path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "original",
            "manual recovery restores original"
        );
        assert!(!bak.exists());
    }
}
