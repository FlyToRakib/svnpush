//! Rollback (plan §12.6): the exact bytes of every file Step 3 changes are
//! copied aside first and restored on cancel or failure before Publish.
//!
//! A file is restored only while it still holds what SVNpush wrote, so an
//! edit made during the run (or after a crash, before Discard) is never
//! overwritten. The hash of the written content is kept beside the copies.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::edit::FileEdit;

use super::RunFailure;

/// A snapshot of files under a root folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// The folder the relative paths belong to.
    pub root: PathBuf,
    /// Where the copies are.
    pub dir: PathBuf,
    /// Relative paths captured.
    pub files: Vec<String>,
    /// BLAKE3 of the content SVNpush wrote to each file, by relative path.
    pub written: BTreeMap<String, String>,
}

fn io(action: &'static str, path: &Path, source: &std::io::Error) -> RunFailure {
    RunFailure::io(action, path, source)
}

fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Replaces `target` with `bytes` through a temporary file and a rename, so
/// an interruption never leaves it half written.
fn write_atomically(target: &Path, bytes: &[u8]) -> Result<(), RunFailure> {
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = target.with_file_name(format!(".{name}.svnpush-restore"));
    std::fs::write(&temp, bytes).map_err(|e| io("write", &temp, &e))?;
    std::fs::rename(&temp, target).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        io("restore", target, &e)
    })
}

impl Snapshot {
    /// Copies the current content of every edited file into `dir`.
    pub fn take(root: &Path, dir: &Path, edits: &[FileEdit]) -> Result<Self, RunFailure> {
        let mut snapshot = Self {
            root: root.to_path_buf(),
            dir: dir.to_path_buf(),
            files: Vec::new(),
            written: BTreeMap::new(),
        };
        std::fs::create_dir_all(dir).map_err(|e| io("create", dir, &e))?;
        snapshot.include(edits)?;
        Ok(snapshot)
    }

    /// The file beside the snapshot folder that holds the written hashes.
    fn manifest(dir: &Path) -> PathBuf {
        dir.with_extension("json")
    }

    /// Captures edited files not captured yet; files already captured keep
    /// their original bytes. Every edit updates what SVNpush last wrote.
    pub fn include(&mut self, edits: &[FileEdit]) -> Result<(), RunFailure> {
        for edit in edits {
            if !self.files.contains(&edit.path) {
                let target = self.dir.join(&edit.path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| io("create", parent, &e))?;
                }
                std::fs::write(&target, edit.before.as_bytes())
                    .map_err(|e| io("write", &target, &e))?;
                self.files.push(edit.path.clone());
            }
            self.written.insert(edit.path.clone(), hash(edit.after.as_bytes()));
        }
        let manifest = Self::manifest(&self.dir);
        let text = serde_json::to_string_pretty(&self.written)
            .map_err(|e| RunFailure::new("RUN_IO", e.to_string(), None))?;
        std::fs::write(&manifest, text).map_err(|e| io("write", &manifest, &e))
    }

    /// Writes the captured bytes back over the edited files that still hold
    /// what SVNpush wrote. Returns the files left alone because they changed
    /// since.
    pub fn restore(&self) -> Result<Vec<String>, RunFailure> {
        let mut skipped = Vec::new();
        for rel in &self.files {
            let saved = self.dir.join(rel);
            let target = self.root.join(rel);
            let original = std::fs::read(&saved).map_err(|e| io("read", &saved, &e))?;
            let current = std::fs::read(&target).ok();
            if current.as_deref() == Some(original.as_slice()) {
                continue;
            }
            let ours = match (self.written.get(rel), &current) {
                (Some(expected), Some(bytes)) => *expected == hash(bytes),
                // A file that is gone was changed by someone else too.
                (Some(_), None) => false,
                // Snapshots from before the hashes were kept.
                (None, _) => true,
            };
            if ours {
                write_atomically(&target, &original)?;
            } else {
                skipped.push(rel.clone());
            }
        }
        Ok(skipped)
    }

    /// Loads a snapshot folder written earlier (for Discard after a crash).
    pub fn open(root: &Path, dir: &Path) -> Result<Self, RunFailure> {
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(dir).min_depth(1) {
            let entry = entry.map_err(|e| RunFailure::new("RUN_IO", e.to_string(), None))?;
            if entry.file_type().is_file() {
                let rel = entry.path().strip_prefix(dir).unwrap_or(entry.path());
                files.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        let written = std::fs::read(Self::manifest(dir))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Ok(Self { root: root.to_path_buf(), dir: dir.to_path_buf(), files, written })
    }

    /// One notice per file [`Snapshot::restore`] left alone.
    pub fn skipped_notices(&self, skipped: &[String]) -> Vec<String> {
        skipped
            .iter()
            .map(|rel| {
                format!(
                    "{rel} changed after SVNpush edited it, so it was not restored. The original is in {}.",
                    self.dir.join(rel).display()
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(path: &str, before: &str, after: &str) -> FileEdit {
        FileEdit { path: path.into(), before: before.into(), after: after.into() }
    }

    #[test]
    fn restores_exact_bytes() {
        let project = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let original = "line one\r\nline two\n\u{feff}";
        std::fs::create_dir_all(project.path().join("inc")).unwrap();
        std::fs::write(project.path().join("inc/a.php"), original).unwrap();
        let edits = vec![edit("inc/a.php", original, "changed")];

        let snap = Snapshot::take(project.path(), &store.path().join("snapshot"), &edits).unwrap();
        std::fs::write(project.path().join("inc/a.php"), "changed").unwrap();
        assert!(snap.restore().unwrap().is_empty());
        assert_eq!(std::fs::read_to_string(project.path().join("inc/a.php")).unwrap(), original);

        let reopened = Snapshot::open(project.path(), &store.path().join("snapshot")).unwrap();
        assert_eq!(reopened.files, ["inc/a.php"]);
        assert_eq!(reopened.written, snap.written);
    }

    #[test]
    fn a_file_edited_since_is_left_alone() {
        let project = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("readme.txt"), "v1").unwrap();
        let mut snap = Snapshot::take(
            project.path(),
            &store.path().join("snapshot"),
            &[edit("readme.txt", "v1", "v2")],
        )
        .unwrap();
        // An applied fix writes the file again; restore still recognises it.
        snap.include(&[edit("readme.txt", "v2", "v3")]).unwrap();
        std::fs::write(project.path().join("readme.txt"), "v3").unwrap();
        let reopened = Snapshot::open(project.path(), &store.path().join("snapshot")).unwrap();
        std::fs::write(project.path().join("readme.txt"), "v3 plus my edit").unwrap();
        assert_eq!(reopened.restore().unwrap(), ["readme.txt"]);
        assert_eq!(
            std::fs::read_to_string(project.path().join("readme.txt")).unwrap(),
            "v3 plus my edit"
        );
        std::fs::write(project.path().join("readme.txt"), "v3").unwrap();
        assert!(reopened.restore().unwrap().is_empty());
        assert_eq!(std::fs::read_to_string(project.path().join("readme.txt")).unwrap(), "v1");
    }
}
