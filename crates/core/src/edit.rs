//! In-memory file edits, composed before anything touches the disk.
//!
//! Step 3 (Write) changes several files, sometimes the same file from two
//! sources (a version constant inside the main plugin file). Every change is
//! applied to an [`EditSet`] first, so the diff can be shown, snapshotted and
//! journalled, and only then written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::error::Coded;

/// A planned change to one file, relative to the package root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FileEdit {
    /// Path relative to the package root, with `/` separators.
    pub path: String,
    /// The file content before the edit.
    pub before: String,
    /// The file content after the edit.
    pub after: String,
}

/// Failure to load or save a file taking part in an edit.
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    /// The file could not be read.
    #[error("could not read {path}: {source}")]
    Read { path: String, source: std::io::Error },
    /// The file is not valid UTF-8, so it cannot be edited as text.
    #[error("{path} is not valid UTF-8 text")]
    NotUtf8 { path: String },
    /// The file could not be written.
    #[error("could not write {path}: {source}")]
    Write { path: String, source: std::io::Error },
}

impl Coded for EditError {
    fn code(&self) -> &'static str {
        match self {
            Self::Read { .. } => "EDIT_READ_FAILED",
            Self::NotUtf8 { .. } => "EDIT_NOT_UTF8",
            Self::Write { .. } => "EDIT_WRITE_FAILED",
        }
    }

    fn fix(&self) -> Option<String> {
        match self {
            Self::NotUtf8 { path } => Some(format!("Save {path} as UTF-8.")),
            Self::Read { .. } | Self::Write { .. } => None,
        }
    }
}

/// A set of in-memory edits keyed by relative path.
#[derive(Debug)]
pub struct EditSet {
    root: PathBuf,
    files: BTreeMap<String, FileEdit>,
}

impl EditSet {
    /// An empty set of edits under `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), files: BTreeMap::new() }
    }

    /// The package root the relative paths resolve against.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The current (possibly already edited) content of `rel`, loading it on first use.
    pub fn current(&mut self, rel: &str) -> Result<&str, EditError> {
        if !self.files.contains_key(rel) {
            let content = read_text(&self.root, rel)?;
            self.files.insert(
                rel.to_owned(),
                FileEdit { path: rel.to_owned(), before: content.clone(), after: content },
            );
        }
        Ok(self.files.get(rel).map_or("", |f| f.after.as_str()))
    }

    /// Replaces the content of `rel` with the result of `change`.
    pub fn modify<E>(
        &mut self,
        rel: &str,
        change: impl FnOnce(&str) -> Result<String, E>,
    ) -> Result<(), E>
    where
        E: From<EditError>,
    {
        let next = change(self.current(rel)?)?;
        if let Some(file) = self.files.get_mut(rel) {
            file.after = next;
        }
        Ok(())
    }

    /// Every file whose content actually changes, sorted by path.
    pub fn changed(&self) -> Vec<FileEdit> {
        self.files.values().filter(|f| f.before != f.after).cloned().collect()
    }

    /// Writes every changed file to disk.
    pub fn write(&self) -> Result<Vec<FileEdit>, EditError> {
        let changed = self.changed();
        for file in &changed {
            let path = self.root.join(&file.path);
            std::fs::write(&path, file.after.as_bytes())
                .map_err(|source| EditError::Write { path: file.path.clone(), source })?;
        }
        Ok(changed)
    }
}

/// Reads `rel` under `root` as UTF-8 text.
pub fn read_text(root: &Path, rel: &str) -> Result<String, EditError> {
    let bytes = std::fs::read(root.join(rel))
        .map_err(|source| EditError::Read { path: rel.to_owned(), source })?;
    String::from_utf8(bytes).map_err(|_| EditError::NotUtf8 { path: rel.to_owned() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_two_changes_to_one_file_and_writes_once() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "one two").unwrap();
        let mut set = EditSet::new(dir.path());
        set.modify::<EditError>("a.txt", |t| Ok(t.replace("one", "1"))).unwrap();
        set.modify::<EditError>("a.txt", |t| Ok(t.replace("two", "2"))).unwrap();
        let changed = set.changed();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].before, "one two");
        assert_eq!(changed[0].after, "1 2");
        set.write().unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "1 2");
    }

    #[test]
    fn unchanged_files_are_not_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "same").unwrap();
        let mut set = EditSet::new(dir.path());
        set.modify::<EditError>("a.txt", |t| Ok(t.to_owned())).unwrap();
        assert!(set.changed().is_empty());
    }

    #[test]
    fn non_utf8_is_a_typed_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.bin"), [0xff, 0xfe, 0x00]).unwrap();
        let err = read_text(dir.path(), "b.bin").unwrap_err();
        assert_eq!(err.code(), "EDIT_NOT_UTF8");
    }
}
