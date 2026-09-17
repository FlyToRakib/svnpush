//! Copying the listing into the staged tree, hashing as it goes.

use std::io::{Read, Write};
use std::path::Path;

use super::walk::Listing;
use super::{PackageError, PackagedFile, io_error};

const BUFFER_BYTES: usize = 64 * 1024;

/// Copies every listed file into `dest` (replacing any previous staging)
/// and returns the files with their BLAKE3 hashes.
pub fn stage(listing: &Listing, dest: &Path) -> Result<Vec<PackagedFile>, PackageError> {
    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(|e| io_error("remove", dest, e))?;
    }
    std::fs::create_dir_all(dest).map_err(|e| io_error("create", dest, e))?;

    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut out = Vec::with_capacity(listing.files.len());
    for file in &listing.files {
        let target = dest.join(&file.rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_error("create", parent, e))?;
        }
        let source = Path::new(&file.abs);
        let mut reader = std::fs::File::open(source).map_err(|e| io_error("open", source, e))?;
        let mut writer =
            std::fs::File::create(&target).map_err(|e| io_error("create", &target, e))?;
        let mut hasher = blake3::Hasher::new();
        let mut size = 0_u64;
        loop {
            let read = reader.read(&mut buffer).map_err(|e| io_error("read", source, e))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            writer.write_all(&buffer[..read]).map_err(|e| io_error("write", &target, e))?;
            size += read as u64;
        }
        out.push(PackagedFile {
            rel: file.rel.clone(),
            size,
            hash: hasher.finalize().to_hex().to_string(),
        });
    }
    Ok(out)
}

/// The BLAKE3 hash of a file, lowercase hex.
pub fn hash_file(path: &Path) -> Result<String, PackageError> {
    let mut reader = std::fs::File::open(path).map_err(|e| io_error("open", path, e))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    loop {
        let read = reader.read(&mut buffer).map_err(|e| io_error("read", path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{Exclusions, list};

    #[test]
    fn stages_files_with_hashes_and_replaces_old_staging() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(src.path().join("inc")).unwrap();
        std::fs::write(src.path().join("inc/a.php"), "hello").unwrap();
        let dest = out.path().join("slug");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("stale.txt"), "old").unwrap();

        let ex = Exclusions::load(src.path(), &[]).unwrap();
        let files = stage(&list(src.path(), &ex).unwrap(), &dest).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].size, 5);
        assert_eq!(files[0].hash, blake3::hash(b"hello").to_hex().to_string());
        assert_eq!(hash_file(&dest.join("inc/a.php")).unwrap(), files[0].hash);
        assert!(!dest.join("stale.txt").exists());
    }
}
