//! The deterministic zip and its SHA-256.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use super::{PackageError, PackagedFile, io_error};

fn zip_error(err: &impl std::fmt::Display) -> PackageError {
    PackageError::Zip { reason: err.to_string() }
}

/// Writes `files` (staged under `staged_root`) to `zip_path`.
///
/// Every entry sits under `<slug>/`, uses `/` separators, and is written in
/// sorted order with a fixed timestamp and permissions, so the same tree
/// always produces the same bytes.
pub fn write_zip(
    staged_root: &Path,
    files: &[PackagedFile],
    slug: &str,
    zip_path: &Path,
) -> Result<(), PackageError> {
    if let Some(parent) = zip_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_error("create", parent, e))?;
    }
    let file = std::fs::File::create(zip_path).map_err(|e| io_error("create", zip_path, e))?;
    let mut zip = ZipWriter::new(file);
    let base = SimpleFileOptions::default()
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    let file_options = base.compression_method(CompressionMethod::Deflated);
    let dir_options = base.compression_method(CompressionMethod::Stored).unix_permissions(0o755);

    let mut dirs = BTreeSet::new();
    dirs.insert(format!("{slug}/"));
    for f in files {
        let mut prefix = String::new();
        let parts: Vec<&str> = f.rel.split('/').collect();
        for part in &parts[..parts.len().saturating_sub(1)] {
            prefix.push_str(part);
            prefix.push('/');
            dirs.insert(format!("{slug}/{prefix}"));
        }
    }

    let mut entries: Vec<(String, Option<&PackagedFile>)> =
        dirs.into_iter().map(|d| (d, None)).collect();
    entries.extend(files.iter().map(|f| (format!("{slug}/{}", f.rel), Some(f))));
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut buffer = Vec::new();
    for (name, file) in entries {
        match file {
            None => zip.add_directory(name, dir_options).map_err(|e| zip_error(&e))?,
            Some(f) => {
                zip.start_file(name, file_options).map_err(|e| zip_error(&e))?;
                let source = staged_root.join(&f.rel);
                buffer.clear();
                std::fs::File::open(&source)
                    .and_then(|mut r| r.read_to_end(&mut buffer))
                    .map_err(|e| io_error("read", &source, e))?;
                zip.write_all(&buffer).map_err(|e| zip_error(&e))?;
            }
        }
    }
    zip.finish().map_err(|e| zip_error(&e))?;
    Ok(())
}

/// Every entry name in a zip, in archive order (check V13).
pub fn zip_entry_names(zip_path: &Path) -> Result<Vec<String>, PackageError> {
    let file = std::fs::File::open(zip_path).map_err(|e| io_error("open", zip_path, e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| zip_error(&e))?;
    (0..archive.len())
        .map(|i| {
            archive.by_index_raw(i).map(|entry| entry.name().to_owned()).map_err(|e| zip_error(&e))
        })
        .collect()
}

/// SHA-256 of a file, lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String, PackageError> {
    let mut reader = std::fs::File::open(path).map_err(|e| io_error("open", path, e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|e| io_error("read", path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().iter().fold(String::with_capacity(64), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    }))
}
