//! Step 5, Build: exclusion rules, staging, the zip and its checksum.

mod archive;
pub mod rules;
mod stage;
mod walk;

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::error::Coded;

pub use archive::{sha256_file, write_zip, zip_entry_names};
pub use rules::{DEFAULT_EXCLUDES, ExclusionSource, Exclusions, HARD_EXCLUDES, gitignored};
pub use stage::{hash_file, stage};
pub use walk::{ListedFile, Listing, list};

/// Paths longer than this warn: some Windows tooling still rejects them.
pub const LONG_PATH_CHARS: usize = 260;

/// How many built versions per plugin are kept on disk.
pub const BUILDS_KEPT: usize = 3;

/// One file in the staged package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PackagedFile {
    /// Path relative to the package root, with `/` separators.
    pub rel: String,
    /// Size in bytes.
    #[ts(type = "number")]
    pub size: u64,
    /// BLAKE3 hash of the content, lowercase hex.
    pub hash: String,
}

/// A built package: the staged tree, the zip and its SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Package {
    /// Absolute path of the staged `<slug>/` folder.
    pub root: String,
    /// Every staged file, sorted by path.
    pub files: Vec<PackagedFile>,
    /// Sum of all file sizes in bytes.
    #[ts(type = "number")]
    pub total_size: u64,
    /// Absolute path of `<slug>-<version>.zip`.
    pub zip_path: String,
    /// SHA-256 of the zip, lowercase hex.
    pub sha256: String,
    /// Which exclusion list was used.
    pub exclusion_source: ExclusionSource,
    /// Source paths longer than [`LONG_PATH_CHARS`].
    pub long_paths: Vec<String>,
}

/// A packaging failure.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    /// A symbolic link points back into its own ancestry.
    #[error("symbolic link loop at {path}")]
    SymlinkLoop { path: String },
    /// A file or folder name is not valid UTF-8.
    #[error("the name of {path} is not valid UTF-8")]
    NameNotUtf8 { path: String },
    /// A name cannot be written on Windows.
    #[error("{path} cannot be written on Windows: {reason}")]
    InvalidName { path: String, reason: String },
    /// `.distignore` could not be parsed.
    #[error(".distignore line is invalid: {reason}")]
    BadDistignore { reason: String },
    /// A filesystem operation failed.
    #[error("{action} {path}: {source}")]
    Io { action: &'static str, path: String, source: std::io::Error },
    /// Writing the zip failed.
    #[error("could not write the zip: {reason}")]
    Zip { reason: String },
}

impl Coded for PackageError {
    fn code(&self) -> &'static str {
        match self {
            Self::SymlinkLoop { .. } => "PACKAGE_SYMLINK_LOOP",
            Self::NameNotUtf8 { .. } => "PACKAGE_NAME_NOT_UTF8",
            Self::InvalidName { .. } => "PACKAGE_INVALID_NAME",
            Self::BadDistignore { .. } => "PACKAGE_BAD_DISTIGNORE",
            Self::Io { .. } => "PACKAGE_IO",
            Self::Zip { .. } => "PACKAGE_ZIP",
        }
    }

    fn fix(&self) -> Option<String> {
        match self {
            Self::SymlinkLoop { path } => {
                Some(format!("Remove the link at {path} or exclude it in .distignore."))
            }
            Self::NameNotUtf8 { path } | Self::InvalidName { path, .. } => {
                Some(format!("Rename {path} or exclude it in .distignore."))
            }
            Self::BadDistignore { .. } => Some("Correct the pattern in .distignore.".to_owned()),
            Self::Io { .. } | Self::Zip { .. } => None,
        }
    }
}

pub(crate) fn io_error(action: &'static str, path: &Path, source: std::io::Error) -> PackageError {
    PackageError::Io { action, path: path.display().to_string(), source }
}

/// Where a version's build output lives: `<builds>/<slug>/<version>/`.
pub fn build_dir(builds: &Path, slug: &str, version: &str) -> PathBuf {
    builds.join(slug).join(version)
}

/// Stages the listing, writes the zip and its `.sha256` file.
pub fn build(
    listing: &Listing,
    builds: &Path,
    slug: &str,
    version: &str,
) -> Result<Package, PackageError> {
    let out = build_dir(builds, slug, version);
    let staged_root = out.join(slug);
    let files = stage(listing, &staged_root)?;
    let zip_path = out.join(format!("{slug}-{version}.zip"));
    write_zip(&staged_root, &files, slug, &zip_path)?;
    let sha256 = sha256_file(&zip_path)?;
    let checksum_path = out.join(format!("{slug}-{version}.zip.sha256"));
    std::fs::write(&checksum_path, format!("{sha256}  {slug}-{version}.zip\n"))
        .map_err(|e| io_error("write", &checksum_path, e))?;

    Ok(Package {
        root: staged_root.display().to_string(),
        total_size: files.iter().map(|f| f.size).sum(),
        files,
        zip_path: zip_path.display().to_string(),
        sha256,
        exclusion_source: listing.exclusion_source,
        long_paths: listing.long_paths.clone(),
    })
}

/// Deletes all but the newest [`BUILDS_KEPT`] version folders for `slug`.
pub fn prune_builds(builds: &Path, slug: &str) -> Result<(), PackageError> {
    let dir = builds.join(slug);
    if !dir.is_dir() {
        return Ok(());
    }
    let mut versions = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| io_error("read", &dir, e))? {
        let entry = entry.map_err(|e| io_error("read", &dir, e))?;
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .map_err(|e| io_error("inspect", &entry.path(), e))?;
        if entry.path().is_dir() {
            versions.push((modified, entry.path()));
        }
    }
    versions.sort_by_key(|v| std::cmp::Reverse(v.0));
    for (_, path) in versions.into_iter().skip(BUILDS_KEPT) {
        std::fs::remove_dir_all(&path).map_err(|e| io_error("remove", &path, e))?;
    }
    Ok(())
}
