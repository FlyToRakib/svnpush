//! Walking the package root into a validated file listing.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use super::rules::{ExclusionSource, Exclusions};
use super::{LONG_PATH_CHARS, PackageError};

/// One file that will be packaged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ListedFile {
    /// Path relative to the package root, with `/` separators.
    pub rel: String,
    /// Absolute source path.
    pub abs: String,
    /// Size in bytes.
    #[ts(type = "number")]
    pub size: u64,
}

/// The files a package root will produce, before staging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Listing {
    /// Absolute package root.
    pub root: String,
    /// Every included file, sorted by relative path.
    pub files: Vec<ListedFile>,
    /// Which exclusion list decided.
    pub exclusion_source: ExclusionSource,
    /// Source paths longer than the long-path limit.
    pub long_paths: Vec<String>,
}

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Why a single name cannot be written on Windows, if it cannot.
pub fn windows_name_problem(name: &str) -> Option<String> {
    if let Some(c) = name
        .chars()
        .find(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\\') || c.is_control())
    {
        return Some(format!("contains the character {c:?}"));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Some("ends with a dot or a space".to_owned());
    }
    let stem = name.split('.').next().unwrap_or(name);
    if WINDOWS_RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        return Some(format!("{stem} is a reserved name"));
    }
    None
}

fn relative(root: &Path, path: &Path) -> Result<String, PackageError> {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let mut parts = Vec::new();
    for component in rel.components() {
        if let Component::Normal(part) = component {
            let name = part
                .to_str()
                .ok_or_else(|| PackageError::NameNotUtf8 { path: path.display().to_string() })?;
            parts.push(name);
        }
    }
    let joined = parts.join("/");
    for part in &parts {
        if let Some(reason) = windows_name_problem(part) {
            return Err(PackageError::InvalidName { path: joined, reason });
        }
    }
    Ok(joined)
}

/// At most this many left-out paths are reported; the rest are counted.
pub const MAX_EXCLUDED_LISTED: usize = 2000;

/// What the rules keep and what they leave out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    /// The files that will be packaged.
    pub listing: Listing,
    /// Left-out paths relative to the root; a left-out folder is listed once
    /// with a trailing `/`. At most [`MAX_EXCLUDED_LISTED`].
    pub excluded: Vec<String>,
    /// How many left-out paths there are in total.
    pub excluded_total: usize,
}

/// Lists every file under `root` that the exclusion rules keep.
///
/// Symbolic links are followed and their targets listed as files; a loop is
/// an error. Empty folders produce nothing.
pub fn list(root: &Path, exclusions: &Exclusions) -> Result<Listing, PackageError> {
    split(root, exclusions).map(|s| s.listing)
}

fn lossy_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

/// Walks `root` once, sorting every entry into kept files and left-out
/// paths. A left-out folder is not entered.
pub fn split(root: &Path, exclusions: &Exclusions) -> Result<Split, PackageError> {
    let mut files = Vec::new();
    let mut long_paths = Vec::new();
    let mut excluded = Vec::new();
    let mut excluded_total = 0;
    let mut walker =
        walkdir::WalkDir::new(root).follow_links(true).min_depth(1).sort_by_file_name().into_iter();

    while let Some(entry) = walker.next() {
        let entry = entry.map_err(|err| {
            let path =
                err.path().map_or_else(|| root.display().to_string(), |p| p.display().to_string());
            if err.loop_ancestor().is_some() {
                PackageError::SymlinkLoop { path }
            } else {
                let source =
                    err.into_io_error().unwrap_or_else(|| std::io::Error::other("walk failed"));
                PackageError::Io { action: "read", path, source }
            }
        })?;
        let is_dir = entry.file_type().is_dir();
        if exclusions.is_excluded(entry.path(), is_dir) {
            excluded_total += 1;
            if excluded.len() < MAX_EXCLUDED_LISTED {
                let rel = lossy_relative(root, entry.path());
                excluded.push(if is_dir { format!("{rel}/") } else { rel });
            }
            if is_dir {
                walker.skip_current_dir();
            }
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let path: PathBuf = entry.path().to_path_buf();
        let rel = relative(root, &path)?;
        let size = entry
            .metadata()
            .map_err(|e| PackageError::Io {
                action: "inspect",
                path: path.display().to_string(),
                source: e
                    .into_io_error()
                    .unwrap_or_else(|| std::io::Error::other("metadata unavailable")),
            })?
            .len();
        let abs = path.display().to_string();
        if abs.chars().count() > LONG_PATH_CHARS {
            long_paths.push(rel.clone());
        }
        files.push(ListedFile { rel, abs, size });
    }
    files.sort_by(|a, b| a.rel.cmp(&b.rel));

    Ok(Split {
        listing: Listing {
            root: root.display().to_string(),
            files,
            exclusion_source: exclusions.source(),
            long_paths,
        },
        excluded,
        excluded_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_names() {
        assert!(windows_name_problem("plugin.php").is_none());
        assert!(windows_name_problem("a:b.php").is_some());
        assert!(windows_name_problem("con.txt").is_some());
        assert!(windows_name_problem("trailing.").is_some());
        assert!(windows_name_problem("console.php").is_none());
    }

    #[test]
    fn lists_sorted_files_and_skips_excluded_and_empty_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("includes")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(root.join("empty")).unwrap();
        std::fs::write(root.join("plugin.php"), "<?php").unwrap();
        std::fs::write(root.join("includes/b.php"), "b").unwrap();
        std::fs::write(root.join("includes/a.php"), "aa").unwrap();
        std::fs::write(root.join("node_modules/pkg/index.js"), "x").unwrap();
        let ex = Exclusions::load(root, &[]).unwrap();
        let listing = list(root, &ex).unwrap();
        let rels: Vec<&str> = listing.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(rels, ["includes/a.php", "includes/b.php", "plugin.php"]);
        assert_eq!(listing.files[0].size, 2);
    }

    #[test]
    fn split_reports_left_out_folders_once_and_files_individually() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".agent/notes")).unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join(".agent/notes/a.md"), "a").unwrap();
        std::fs::write(root.join("docs/guide.md"), "g").unwrap();
        std::fs::write(root.join("README.md"), "r").unwrap();
        std::fs::write(root.join("plugin.php"), "<?php").unwrap();
        let ex = Exclusions::load(root, &[]).unwrap();
        let result = split(root, &ex).unwrap();
        let kept: Vec<&str> = result.listing.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(kept, ["plugin.php"]);
        assert_eq!(result.excluded, [".agent/", "README.md", "docs/"]);
        assert_eq!(result.excluded_total, 3);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_loop_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::os::unix::fs::symlink(root.join("a"), root.join("a/loop")).unwrap();
        let ex = Exclusions::load(root, &[]).unwrap();
        let err = list(root, &ex).unwrap_err();
        assert!(matches!(err, PackageError::SymlinkLoop { .. }));
    }
}
