//! Mirroring a folder into a working-copy folder (plan §8.3).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::package::{self, Exclusions, PackageError, PackagedFile};

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

/// Versioned folders under `dir` that no longer hold any file, outermost only.
fn empty_folders(dir: &Path) -> Vec<PathBuf> {
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
    out
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
    /// compared by content hash. `svn:eol-style` is never set.
    pub async fn mirror(&self, sources: &[SourceFile], target: &Path) -> Result<Delta, SvnError> {
        if !target.is_dir() {
            let mut args: Vec<OsString> = vec!["mkdir".into()];
            args.push(target.as_os_str().to_owned());
            self.local(args).await?;
        }

        let existing = existing_files(target)?;
        let by_lower: HashMap<String, &String> =
            existing.keys().map(|k| (k.to_lowercase(), k)).collect();
        let wanted: BTreeSet<&str> = sources.iter().map(|s| s.rel.as_str()).collect();

        let mut delta = Delta::default();
        let mut to_add = Vec::new();
        let mut to_delete = Vec::new();

        for source in sources {
            if let Some(current) = existing.get(&source.rel) {
                let hash = package::hash_file(current)
                    .map_err(|e| io_error("hash", current, std::io::Error::other(e.to_string())))?;
                if hash != source.hash {
                    delta.modified.push(source.rel.clone());
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
        for rel in existing.keys() {
            if !wanted.contains(rel.as_str()) {
                delta.deleted.push(rel.clone());
                to_delete.push(target.join(rel));
            }
        }

        if !to_delete.is_empty() {
            self.reporter
                .info(&format!("Deleting {} file(s) no longer in the package.", to_delete.len()));
            self.batched(&["delete", "--force"], &to_delete).await?;
        }

        for source in sources {
            if delta.added.contains(&source.rel) || delta.modified.contains(&source.rel) {
                copy(&source.abs, &target.join(&source.rel))?;
            }
        }
        for rel in &delta.added {
            to_add.push(target.join(rel));
        }
        if !to_add.is_empty() {
            self.reporter.info(&format!("Adding {} new file(s).", to_add.len()));
            self.batched(
                &["add", "--force", "--parents", "--no-auto-props", "--no-ignore"],
                &to_add,
            )
            .await?;
        }

        let emptied = empty_folders(target);
        if !emptied.is_empty() {
            for folder in &emptied {
                delta
                    .deleted
                    .push(format!("{}/", slash(folder.strip_prefix(target).unwrap_or(folder))));
            }
            self.batched(&["delete", "--force"], &emptied).await?;
        }

        let mut by_mime: BTreeMap<&str, Vec<PathBuf>> = BTreeMap::new();
        for rel in delta.added.iter().chain(&delta.modified) {
            if let Some(mime) = mime_type(rel) {
                by_mime.entry(mime).or_default().push(target.join(rel));
            }
        }
        for (mime, paths) in by_mime {
            self.batched(&["propset", "svn:mime-type", mime], &paths).await?;
        }

        delta.added.sort();
        delta.modified.sort();
        delta.deleted.sort();
        Ok(delta)
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
        let found = empty_folders(dir.path());
        assert_eq!(found, [dir.path().join("gone")]);
    }
}
