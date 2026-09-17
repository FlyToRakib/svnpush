//! One release per plugin at a time, across windows and app instances
//! (plan §10). The lock is an operating system file lock, so a crashed app
//! never leaves a stale lock behind.

use std::fs::{File, TryLockError};
use std::path::Path;

use super::RunFailure;

/// Holds the lock until dropped.
#[derive(Debug)]
pub struct ProjectLock {
    _file: File,
}

impl ProjectLock {
    /// Takes the lock for the runs folder `dir`, or reports that another run holds it.
    pub fn acquire(dir: &Path) -> Result<Self, RunFailure> {
        std::fs::create_dir_all(dir).map_err(|e| RunFailure::io("create", dir, &e))?;
        let path = dir.join(".lock");
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| RunFailure::io("open", &path, &e))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(RunFailure::new(
                "RUN_LOCKED",
                "Another SVNpush window is already releasing this plugin.",
                Some("Wait for that release to finish, or close the other window.".to_owned()),
            )),
            Err(TryLockError::Error(e)) => Err(RunFailure::io("lock", &path, &e)),
        }
    }

    /// Whether another run currently holds the lock for `dir`.
    pub fn is_held(dir: &Path) -> bool {
        let path = dir.join(".lock");
        let Ok(file) = File::options().write(true).open(&path) else { return false };
        match file.try_lock() {
            Ok(()) => {
                let _ = file.unlock();
                false
            }
            Err(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_fails_until_the_first_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let first = ProjectLock::acquire(dir.path()).unwrap();
        assert!(ProjectLock::is_held(dir.path()));
        let second = ProjectLock::acquire(dir.path()).unwrap_err();
        assert_eq!(second.error.code, "RUN_LOCKED");
        drop(first);
        assert!(!ProjectLock::is_held(dir.path()));
        assert!(ProjectLock::acquire(dir.path()).is_ok());
    }
}
