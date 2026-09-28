//! Mirroring a folder into a working-copy folder (plan §8.3).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::package::{self, Exclusions, PackageError, PackagedFile};

use super::status::StatusItem;
use super::{Delta, Svn, SvnError, io_error, slash};

/// A file to mirror: where it is, and its content hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Path relative to the source folder, with `/` separators.
    pub rel: String,
    /// Absolute path.
    pub abs: PathBuf,
    /// BLAKE3 hash, lowercase hex.
    pub hash: String,
}

/// Source files for a staged package, reusing the hashes computed during staging.
pub fn source_files(staged_root: &Path, files: &[PackagedFile]) -> Vec<SourceFile> {
    files
        .iter()
        .map(|f| SourceFile {
            rel: f.rel.clone(),
            abs: staged_root.join(&f.rel),
            hash: f.hash.clone(),
        })
        .collect()
}

/// Every file in a listing-less folder (such as `.wordpress-org/`), hashed,
/// with only the always-excluded names removed.
pub fn folder_files(root: &Path) -> Result<Vec<SourceFile>, PackageError> {
    let listing = package::list(root, &Exclusions::hard_only(root)?)?;
    listing
        .files
        .into_iter()
        .map(|f| {
            let abs = PathBuf::from(&f.abs);
            Ok(SourceFile { hash: package::hash_file(&abs)?, rel: f.rel, abs })
        })
        .collect()
}

/// The `svn:mime-type` for binary extensions SVNpush marks (plan §8.3 step 6).
pub fn mime_type(rel: &str) -> Option<&'static str> {
    let ext = rel.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "eot" => "application/vnd.ms-fontobject",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        _ => return None,
    })
}

/// Files under `dir`, skipping `.svn`, as relative path → absolute path.
fn existing_files(dir: &Path) -> Result<BTreeMap<String, PathBuf>, SvnError> {
    let mut out = BTreeMap::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let walker = walkdir::WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".svn");
    for entry in walker {
        let entry = entry.map_err(|e| SvnError::Io {
            action: "read",
            path: dir.display().to_string(),
            source: e.into_io_error().unwrap_or_else(|| std::io::Error::other("walk failed")),
        })?;
        if entry.file_type().is_file() {
            let rel = slash(entry.path().strip_prefix(dir).unwrap_or(entry.path()));
            out.insert(rel, entry.path().to_path_buf());
        }
    }
    Ok(out)
}

/// Versioned folders under `dir` that no longer hold any file, outermost
/// only, relative to `dir`.
fn empty_folders(dir: &Path) -> Vec<String> {
    fn holds_files(path: &Path) -> bool {
        walkdir::WalkDir::new(path)
            .into_iter()
            .filter_entry(|e| e.file_name() != ".svn")
            .filter_map(Result::ok)
            .any(|e| e.file_type().is_file())
    }
    let mut out: Vec<PathBuf> = Vec::new();
    let walker = walkdir::WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".svn");
    for entry in walker.filter_map(Result::ok) {
        let path = entry.path();
        if entry.file_type().is_dir()
            && !out.iter().any(|p| path.starts_with(p))
            && !holds_files(path)
        {
            out.push(path.to_path_buf());
        }
    }
    out.iter().map(|p| slash(p.strip_prefix(dir).unwrap_or(p))).collect()
}

/// The folders of a relative path: `a/b/c.php` → `a`, `a/b`.
fn folders_of(rel: &str) -> impl Iterator<Item = &str> {
    rel.match_indices('/').map(move |(i, _)| &rel[..i])
}

/// Existing folders whose name changed only in case (`Includes/` →
/// `includes/`), outermost only. A case-insensitive file system would keep
/// copying into the old spelling, so these are deleted as a whole first.
fn case_renamed_folders<'a>(
    existing: impl Iterator<Item = &'a String>,
    wanted: &[&str],
) -> Vec<String> {
    let source: HashSet<&str> = wanted.iter().flat_map(|rel| folders_of(rel)).collect();
    let source_lower: HashSet<String> = source.iter().map(|f| f.to_lowercase()).collect();
    let current: BTreeSet<&str> = existing.flat_map(|rel| folders_of(rel)).collect();
    let mut out: Vec<String> = Vec::new();
    for folder in current {
        if !source.contains(folder)
            && source_lower.contains(&folder.to_lowercase())
            && !out.iter().any(|o| folder.starts_with(&format!("{o}/")))
        {
            out.push(folder.to_owned());
        }
    }
    out
}

/// Whether `rel` is `folder` or inside it.
fn within(rel: &str, folder: &str) -> bool {
    rel.strip_prefix(folder).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Whether `svn` can take `rel` as a command-line argument. Windows builds
/// read arguments in the ANSI code page, so other names only work through
/// folder-level operations.
fn addressable(rel: &str) -> bool {
    !cfg!(windows) || rel.is_ascii()
}

fn copy(from: &Path, to: &Path) -> Result<(), SvnError> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_error("create", parent, e))?;
    }
    std::fs::copy(from, to).map(|_| ()).map_err(|e| io_error("copy", from, e))
}

impl Svn<'_> {
    /// Mirrors `sources` into the working-copy folder `target` and schedules
    /// the changes: `svn add` for new files, `svn delete` for vanished files
    /// and emptied folders, and `svn:mime-type` for binaries. Files are
    /// compared by content hash, and a changed hash counts as modified only
    /// when `svn status` agrees (it ignores line-ending and keyword
    /// differences on files with `svn:eol-style` or `svn:keywords`).
    /// `svn:eol-style` is never set.
    pub async fn mirror(&self, sources: &[SourceFile], target: &Path) -> Result<Delta, SvnError> {
        if !target.is_dir() {
            let mut args: Vec<OsString> = vec!["mkdir".into()];
            args.push(target.as_os_str().to_owned());
            self.local(args).await?;
        }

        self.remove_unversioned(target).await?;
        let existing = existing_files(target)?;
        let by_lower: HashMap<String, &String> =
            existing.keys().map(|k| (k.to_lowercase(), k)).collect();
        let wanted: Vec<&str> = sources.iter().map(|s| s.rel.as_str()).collect();
        let wanted_set: HashSet<&str> = wanted.iter().copied().collect();
        let renamed = case_renamed_folders(existing.keys(), &wanted);

        let mut delta = Delta::default();
        let mut changed: Vec<&SourceFile> = Vec::new();
        for source in sources {
            if let Some(current) = existing.get(&source.rel) {
                let hash = package::hash_file(current)
                    .map_err(|e| io_error("hash", current, std::io::Error::other(e.to_string())))?;
                if hash != source.hash {
                    changed.push(source);
                }
                continue;
            }
            // A case-only rename (Foo.php → foo.php) is a delete of the old
            // name plus an add of the new one, so case-insensitive file
            // systems do not keep the old spelling.
            if let Some(old) = by_lower.get(&source.rel.to_lowercase()) {
                self.reporter.info(&format!("Case-only rename: {old} → {}.", source.rel));
            }
            delta.added.push(source.rel.clone());
        }

        let mut to_delete = Vec::new();
        for rel in existing.keys() {
            if !wanted_set.contains(rel.as_str()) {
                delta.deleted.push(rel.clone());
                if !renamed.iter().any(|folder| within(rel, folder)) {
                    to_delete.push(rel.clone());
                }
            }
        }
        if !renamed.is_empty() {
            for folder in &renamed {
                self.reporter.info(&format!("Case-only folder rename: {folder}/."));
                delta.deleted.push(format!("{folder}/"));
            }
            self.batched(&["delete", "--force"], target, &renamed).await?;
        }
        if !to_delete.is_empty() {
            self.reporter
                .info(&format!("Deleting {} file(s) no longer in the package.", to_delete.len()));
            self.batched(&["delete", "--force"], target, &to_delete).await?;
        }

        let added: HashSet<&str> = delta.added.iter().map(String::as_str).collect();
        for source in sources {
            if added.contains(source.rel.as_str()) {
                copy(&source.abs, &target.join(&source.rel))?;
            }
        }
        for source in &changed {
            copy(&source.abs, &target.join(&source.rel))?;
        }
        if !delta.added.is_empty() {
            // One folder-level add: it takes any file name (an argument
            // cannot carry characters outside the Windows code page) and has
            // no command-line length limit.
            self.reporter.info(&format!("Adding {} new file(s).", delta.added.len()));
            let args =
                ["add", "--force", "--depth", "infinity", "--no-auto-props", "--no-ignore", "."];
            self.local_in(target, args.iter().map(OsString::from).collect()).await?;
        }

        let emptied = empty_folders(target);
        if !emptied.is_empty() {
            delta.deleted.extend(emptied.iter().map(|f| format!("{f}/")));
            self.batched(&["delete", "--force"], target, &emptied).await?;
        }

        if !changed.is_empty() {
            let modified: HashSet<String> = self
                .folder_status(target)
                .await?
                .into_iter()
                .filter(|e| matches!(e.item, StatusItem::Modified | StatusItem::Replaced))
                .map(|e| e.path)
                .collect();
            delta.modified = changed
                .iter()
                .map(|s| s.rel.clone())
                .filter(|rel| modified.contains(rel))
                .collect();
        }

        self.mark_binaries(target, &delta).await?;
        delta.added.sort();
        delta.modified.sort();
        delta.deleted.sort();
        Ok(delta)
    }

    /// Removes unversioned and ignored files under `target`. An interrupted
    /// run leaves them behind; they are not on the server, and removing them
    /// makes them count as additions again.
    async fn remove_unversioned(&self, target: &Path) -> Result<(), SvnError> {
        for entry in self.folder_status(target).await? {
            if entry.item != StatusItem::Unversioned || entry.path.is_empty() {
                continue;
            }
            let path = target.join(&entry.path);
            let removed = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            removed.map_err(|e| io_error("remove", &path, e))?;
        }
        Ok(())
    }

    /// Sets `svn:mime-type` on added and modified binaries.
    async fn mark_binaries(&self, target: &Path, delta: &Delta) -> Result<(), SvnError> {
        let mut by_mime: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for rel in delta.added.iter().chain(&delta.modified) {
            if let Some(mime) = mime_type(rel) {
                if addressable(rel) {
                    by_mime.entry(mime).or_default().push(rel.clone());
                } else {
                    // `svn add` already marked it application/octet-stream.
                    self.reporter.info(&format!("{rel} keeps the generic binary type."));
                }
            }
        }
        for (mime, rels) in by_mime {
            self.batched(&["propset", "svn:mime-type", mime], target, &rels).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_types_for_binaries_only() {
        assert_eq!(mime_type("assets/icon-256x256.PNG"), Some("image/png"));
        assert_eq!(mime_type("fonts/a.woff2"), Some("font/woff2"));
        assert_eq!(mime_type("readme.txt"), None);
        assert_eq!(mime_type("Makefile"), None);
    }

    #[test]
    fn existing_files_skip_svn_metadata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".svn/pristine")).unwrap();
        std::fs::create_dir_all(dir.path().join("inc")).unwrap();
        std::fs::write(dir.path().join(".svn/wc.db"), "x").unwrap();
        std::fs::write(dir.path().join("inc/a.php"), "a").unwrap();
        let found = existing_files(dir.path()).unwrap();
        assert_eq!(found.keys().collect::<Vec<_>>(), ["inc/a.php"]);
    }

    #[test]
    fn empty_folders_are_outermost_and_ignore_svn() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("gone/deeper")).unwrap();
        std::fs::create_dir_all(dir.path().join("kept")).unwrap();
        std::fs::write(dir.path().join("kept/a.php"), "a").unwrap();
        std::fs::create_dir_all(dir.path().join(".svn")).unwrap();
        assert_eq!(empty_folders(dir.path()), ["gone"]);
    }

    #[test]
    fn case_only_folder_renames_are_found_outermost_first() {
        let existing: Vec<String> =
            ["Includes/Sub/a.php", "Includes/b.php", "lib/c.php", "Same/d.php"]
                .map(str::to_owned)
                .to_vec();
        let wanted = ["includes/Sub/a.php", "includes/b.php", "lib/c.php", "Same/d.php"];
        assert_eq!(case_renamed_folders(existing.iter(), &wanted), ["Includes"]);
        assert!(within("Includes/b.php", "Includes"));
        assert!(!within("IncludesX/b.php", "Includes"));
        // Both spellings wanted (a case-sensitive file system): nothing to rename.
        let both = ["Includes/b.php", "includes/b.php"];
        assert!(case_renamed_folders(existing.iter(), &both).is_empty());
    }
}
