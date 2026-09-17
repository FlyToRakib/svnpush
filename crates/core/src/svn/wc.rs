//! The sparse working copy: checkout, update, status, revert, cleanup, reset.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::verify::WorkingCopyState;

use super::status::{self, StatusEntry, StatusItem};
use super::{Credentials, Svn, SvnError, io_error};

/// The folders SVNpush keeps at full depth; `tags/` and `branches/` stay empty.
pub const DEEP_FOLDERS: [&str; 2] = ["trunk", "assets"];

fn os(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

impl Svn<'_> {
    fn deep_folders(wc: &Path) -> Vec<PathBuf> {
        DEEP_FOLDERS.iter().map(|f| wc.join(f)).filter(|p| p.is_dir()).collect()
    }

    /// The repository URL a working copy points at.
    pub async fn working_copy_url(&self, wc: &Path) -> Result<String, SvnError> {
        let mut args = os(&["info", "--show-item", "url"]);
        args.push(wc.as_os_str().to_owned());
        Ok(self.local(args).await?.stdout.trim().to_owned())
    }

    /// Checks out `url` sparsely into `wc`, replacing anything there.
    pub async fn checkout(
        &self,
        url: &str,
        wc: &Path,
        credentials: Option<&Credentials>,
    ) -> Result<(), SvnError> {
        if wc.exists() {
            std::fs::remove_dir_all(wc).map_err(|e| io_error("remove", wc, e))?;
        }
        if let Some(parent) = wc.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_error("create", parent, e))?;
        }
        let mut args = os(&["checkout", "--depth", "immediates", url]);
        args.push(wc.as_os_str().to_owned());
        self.network(args, credentials, true).await?;
        for folder in Self::deep_folders(wc) {
            let mut args = os(&["update", "--set-depth", "infinity"]);
            args.push(folder.into_os_string());
            self.network(args, credentials, true).await?;
        }
        Ok(())
    }

    /// Makes sure `wc` is a sparse working copy of `url` and up to date.
    ///
    /// An existing copy is reused and repaired with `svn cleanup`; if that
    /// fails, or the copy points elsewhere, it is checked out again.
    pub async fn ensure_working_copy(
        &self,
        url: &str,
        wc: &Path,
        credentials: Option<&Credentials>,
    ) -> Result<(), SvnError> {
        let reusable = wc.join(".svn").is_dir()
            && self
                .working_copy_url(wc)
                .await
                .is_ok_and(|found| found.trim_end_matches('/') == url.trim_end_matches('/'));
        if !reusable || self.cleanup(wc).await.is_err() {
            self.reporter.info("Checking out a fresh sparse working copy.");
            return self.checkout(url, wc, credentials).await;
        }
        self.update(wc, credentials).await
    }

    /// `svn update` on `trunk/` and `assets/` at full depth, plus the root at immediates.
    pub async fn update(
        &self,
        wc: &Path,
        credentials: Option<&Credentials>,
    ) -> Result<(), SvnError> {
        let mut args = os(&["update", "--depth", "immediates"]);
        args.push(wc.as_os_str().to_owned());
        self.network(args, credentials, true).await?;
        let folders = Self::deep_folders(wc);
        if folders.is_empty() {
            return Ok(());
        }
        let mut args = os(&["update", "--set-depth", "infinity"]);
        args.extend(folders.into_iter().map(PathBuf::into_os_string));
        self.network(args, credentials, true).await?;
        Ok(())
    }

    /// `svn cleanup`, releasing stale locks.
    pub async fn cleanup(&self, wc: &Path) -> Result<(), SvnError> {
        let mut args = os(&["cleanup"]);
        args.push(wc.as_os_str().to_owned());
        self.local(args).await.map(|_| ())
    }

    /// Deletes the working copy and checks it out again (the V15 fix).
    pub async fn reset(
        &self,
        url: &str,
        wc: &Path,
        credentials: Option<&Credentials>,
    ) -> Result<(), SvnError> {
        self.checkout(url, wc, credentials).await
    }

    /// Local status of `trunk/` and `assets/`, paths relative to `wc`.
    pub async fn status(&self, wc: &Path) -> Result<Vec<StatusEntry>, SvnError> {
        self.status_args(wc, false, None).await
    }

    async fn status_args(
        &self,
        wc: &Path,
        remote: bool,
        credentials: Option<&Credentials>,
    ) -> Result<Vec<StatusEntry>, SvnError> {
        let folders = Self::deep_folders(wc);
        if folders.is_empty() {
            return Ok(Vec::new());
        }
        let mut args = os(&["status", "--xml"]);
        if remote {
            args.push("--show-updates".into());
        }
        args.extend(folders.into_iter().map(PathBuf::into_os_string));
        let output = if remote {
            self.network(args, credentials, false).await?
        } else {
            self.exec(args, None, false).await?
        };
        status::parse(&output.stdout, wc)
    }

    /// Conflicts and server-side changes, for check V15.
    pub async fn working_copy_state(
        &self,
        wc: &Path,
        credentials: Option<&Credentials>,
    ) -> Result<WorkingCopyState, SvnError> {
        if !wc.join(".svn").is_dir() {
            return Ok(WorkingCopyState::NotCreated);
        }
        let entries = self.status_args(wc, true, credentials).await?;
        let conflicts: Vec<String> = entries
            .iter()
            .filter(|e| e.item == StatusItem::Conflicted || e.tree_conflict)
            .map(|e| e.path.clone())
            .collect();
        if !conflicts.is_empty() {
            return Ok(WorkingCopyState::Conflicts(conflicts));
        }
        let stale: Vec<String> =
            entries.iter().filter(|e| e.out_of_date).map(|e| e.path.clone()).collect();
        Ok(if stale.is_empty() {
            WorkingCopyState::Clean
        } else {
            WorkingCopyState::OutOfDate(stale)
        })
    }

    /// Undoes every local change in `trunk/` and `assets/`, including files a
    /// sync added, so the next run starts clean.
    pub async fn revert(&self, wc: &Path) -> Result<(), SvnError> {
        let folders = Self::deep_folders(wc);
        if folders.is_empty() {
            return Ok(());
        }
        let mut args = os(&["revert", "--recursive"]);
        args.extend(folders.iter().map(|f| f.as_os_str().to_owned()));
        self.local(args).await?;
        for entry in self.status(wc).await? {
            if entry.item != StatusItem::Unversioned {
                continue;
            }
            let path = wc.join(&entry.path);
            let removed = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            removed.map_err(|e| io_error("remove", &path, e))?;
        }
        Ok(())
    }

    /// The local diff of everything under `folder`, split per file. Keys are
    /// paths relative to `folder` with `/` separators.
    pub async fn diff_files(&self, folder: &Path) -> Result<Vec<(String, String)>, SvnError> {
        let mut args = os(&["diff", "--internal-diff"]);
        args.push(folder.as_os_str().to_owned());
        let output = self.exec(args, None, false).await?.stdout;
        Ok(split_diff(&output, folder))
    }
}

/// Splits `svn diff` output at its `Index: <path>` lines.
pub(crate) fn split_diff(output: &str, folder: &Path) -> Vec<(String, String)> {
    let root = super::slash(folder);
    let root = root.trim_end_matches('/');
    let mut files: Vec<(String, String)> = Vec::new();
    for line in output.split_inclusive('\n') {
        if let Some(path) = line.strip_prefix("Index: ") {
            let full = path.trim_end().replace('\\', "/");
            let rel = full
                .strip_prefix(root)
                .map_or(full.as_str(), |r| r.trim_start_matches('/'))
                .to_owned();
            files.push((rel, String::new()));
        } else if let Some((_, text)) = files.last_mut() {
            text.push_str(line);
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_diff_output_per_file() {
        let output = "Index: C:\\wc\\trunk\\a.php\n===\n--- a\n+++ a\n@@ -1 +1 @@\n-x\n+y\nIndex: C:\\wc\\trunk\\img\\b.png\n===\nCannot display: file marked as a binary type.\n";
        let files = split_diff(output, Path::new(r"C:\wc\trunk"));
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, "a.php");
        assert!(files[0].1.contains("+y\n"));
        assert_eq!(files[1].0, "img/b.png");
        assert!(files[1].1.contains("binary"));
    }
}
