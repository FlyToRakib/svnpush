//! Git facts (plan §5.1 step 6): branch, uncommitted files, last tag and the
//! commits since it. Git is optional; without it every function returns `None`.

use std::ffi::OsString;
use std::path::Path;

use serde::Serialize;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::report::Reporter;
use crate::tools::process::{self, ProcessError, ProcessSpec};

/// Commit subjects read at most, newest first.
pub const MAX_COMMITS: usize = 200;

/// What git knows about the project folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct GitFacts {
    /// Current branch, `None` on a detached head.
    pub branch: Option<String>,
    /// Uncommitted or untracked paths, relative to the repository root.
    pub dirty: Vec<String>,
    /// The newest tag reachable from HEAD.
    pub last_tag: Option<String>,
    /// Commit subjects since `last_tag` (or the most recent ones without a tag).
    pub commits: Vec<String>,
}

/// Runs git in a project folder.
pub struct Git<'a> {
    bin: &'a Path,
    folder: &'a Path,
    reporter: &'a dyn Reporter,
    cancel: &'a CancellationToken,
}

impl<'a> Git<'a> {
    /// A git runner for `folder`.
    pub fn new(
        bin: &'a Path,
        folder: &'a Path,
        reporter: &'a dyn Reporter,
        cancel: &'a CancellationToken,
    ) -> Self {
        Self { bin, folder, reporter, cancel }
    }

    /// `git -C <folder> <args>`; `Ok(None)` when git exits non-zero.
    pub async fn output(&self, args: &[&str]) -> Result<Option<String>, ProcessError> {
        let mut all: Vec<OsString> = vec!["-C".into(), self.folder.as_os_str().to_owned()];
        all.extend(args.iter().map(OsString::from));
        let spec =
            ProcessSpec { program: self.bin, args: all, cwd: None, stdin: None, log_output: false };
        let out = process::run(spec, self.reporter, self.cancel).await?;
        Ok(out.success().then_some(out.stdout))
    }

    /// The facts, or `None` when the folder is not inside a git work tree.
    pub async fn facts(&self) -> Result<Option<GitFacts>, ProcessError> {
        let inside = self.output(&["rev-parse", "--is-inside-work-tree"]).await?;
        if inside.as_deref().map(str::trim) != Some("true") {
            return Ok(None);
        }
        let branch = self
            .output(&["rev-parse", "--abbrev-ref", "HEAD"])
            .await?
            .map(|b| b.trim().to_owned())
            .filter(|b| !b.is_empty() && b != "HEAD");
        let dirty = self
            .output(&["status", "--porcelain=v1", "-z", "--untracked-files=all", "--", "."])
            .await?
            .map(|s| parse_porcelain_z(&s))
            .unwrap_or_default();
        let last_tag = self
            .output(&["describe", "--tags", "--abbrev=0"])
            .await?
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
        let limit = format!("--max-count={MAX_COMMITS}");
        let range = last_tag.as_ref().map(|t| format!("{t}..HEAD"));
        let mut log_args = vec!["log", "--format=%s", limit.as_str()];
        if let Some(r) = &range {
            log_args.push(r);
        }
        log_args.extend(["--", "."]);
        let commits = self
            .output(&log_args)
            .await?
            .map(|s| s.lines().map(str::to_owned).filter(|l| !l.is_empty()).collect())
            .unwrap_or_default();
        Ok(Some(GitFacts { branch, dirty, last_tag, commits }))
    }
}

/// Paths from `git status --porcelain=v1 -z`. A rename carries its old path as
/// a second NUL-separated field, which is skipped.
pub fn parse_porcelain_z(output: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut fields = output.split('\0');
    while let Some(field) = fields.next() {
        if field.len() < 4 {
            continue;
        }
        let (status, path) = field.split_at(3);
        paths.push(path.to_owned());
        if status.starts_with('R') || status.starts_with('C') {
            fields.next();
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_paths_including_renames() {
        let out = " M plugin.php\0?? new file.txt\0R  new.php\0old.php\0";
        assert_eq!(parse_porcelain_z(out), ["plugin.php", "new file.txt", "new.php"]);
        assert!(parse_porcelain_z("").is_empty());
    }
}
