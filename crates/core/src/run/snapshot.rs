//! Rollback (plan §12.6): the exact bytes of every file Step 3 changes are
//! copied aside first and restored on cancel or failure before Publish.

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
}

fn io(action: &'static str, path: &Path, source: &std::io::Error) -> RunFailure {
    RunFailure::io(action, path, source)
}

impl Snapshot {
    /// Copies the current content of every edited file into `dir`.
    pub fn take(root: &Path, dir: &Path, edits: &[FileEdit]) -> Result<Self, RunFailure> {
        std::fs::create_dir_all(dir).map_err(|e| io("create", dir, &e))?;
        let mut files = Vec::new();
        for edit in edits {
            let target = dir.join(&edit.path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| io("create", parent, &e))?;
            }
            std::fs::write(&target, edit.before.as_bytes())
                .map_err(|e| io("write", &target, &e))?;
            files.push(edit.path.clone());
        }
        Ok(Self { root: root.to_path_buf(), dir: dir.to_path_buf(), files })
    }

    /// Writes the captured bytes back over the edited files.
    pub fn restore(&self) -> Result<(), RunFailure> {
        for rel in &self.files {
            let saved = self.dir.join(rel);
            let target = self.root.join(rel);
            let bytes = std::fs::read(&saved).map_err(|e| io("read", &saved, &e))?;
            std::fs::write(&target, bytes).map_err(|e| io("restore", &target, &e))?;
        }
        Ok(())
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
        Ok(Self { root: root.to_path_buf(), dir: dir.to_path_buf(), files })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_exact_bytes() {
        let project = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let original = "line one\r\nline two\n\u{feff}";
        std::fs::create_dir_all(project.path().join("inc")).unwrap();
        std::fs::write(project.path().join("inc/a.php"), original).unwrap();
        let edits = vec![FileEdit {
            path: "inc/a.php".into(),
            before: original.into(),
            after: "changed".into(),
        }];

        let snap = Snapshot::take(project.path(), &store.path().join("snapshot"), &edits).unwrap();
        std::fs::write(project.path().join("inc/a.php"), "changed").unwrap();
        snap.restore().unwrap();
        assert_eq!(std::fs::read_to_string(project.path().join("inc/a.php")).unwrap(), original);

        let reopened = Snapshot::open(project.path(), &store.path().join("snapshot")).unwrap();
        assert_eq!(reopened.files, ["inc/a.php"]);
    }
}
