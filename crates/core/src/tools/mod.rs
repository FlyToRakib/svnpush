//! Finding `svn` and `git`, and gating on the Subversion version.

pub mod process;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::report::NullReporter;
use crate::verify::{MIN_SVN, svn_major_minor};

pub use process::{ProcessError, ProcessOutput, ProcessSpec};

/// Which tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum ToolKind {
    /// Subversion.
    Svn,
    /// Git.
    Git,
}

/// What discovery found for one tool (the Doctor report row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ToolReport {
    /// The tool.
    pub kind: ToolKind,
    /// Absolute path, when found.
    pub path: Option<String>,
    /// Version text, when it ran.
    pub version: Option<String>,
    /// Found and new enough.
    pub ok: bool,
    /// One sentence for the Doctor row.
    pub message: String,
    /// Install instructions for this platform, when not ok.
    pub fix: Option<String>,
}

fn executable(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() }
}

/// The places Subversion is usually installed, beyond `PATH` (plan §8.1).
pub fn known_svn_locations() -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![
            PathBuf::from(r"C:\Program Files\TortoiseSVN\bin\svn.exe"),
            PathBuf::from(r"C:\Program Files\SlikSvn\bin\svn.exe"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![PathBuf::from("/opt/homebrew/bin/svn"), PathBuf::from("/usr/local/bin/svn")]
    } else {
        vec![PathBuf::from("/usr/bin/svn"), PathBuf::from("/usr/local/bin/svn")]
    }
}

fn known_git_locations() -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![PathBuf::from(r"C:\Program Files\Git\cmd\git.exe")]
    } else if cfg!(target_os = "macos") {
        vec![PathBuf::from("/opt/homebrew/bin/git"), PathBuf::from("/usr/bin/git")]
    } else {
        vec![PathBuf::from("/usr/bin/git")]
    }
}

/// Platform install instructions for Subversion.
pub fn svn_install_instructions() -> &'static str {
    if cfg!(windows) {
        "Install TortoiseSVN with the command-line tools, run choco install svn, or install SlikSVN."
    } else if cfg!(target_os = "macos") {
        "Run brew install subversion. Xcode no longer ships it."
    } else {
        "Run apt install subversion, or your distribution's equivalent."
    }
}

fn git_install_instructions() -> &'static str {
    if cfg!(windows) {
        "Install Git for Windows from git-scm.com, or run winget install Git.Git."
    } else if cfg!(target_os = "macos") {
        "Run xcode-select --install or brew install git."
    } else {
        "Run apt install git, or your distribution's equivalent."
    }
}

/// Searches `PATH` for an executable.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let file = executable(name);
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .map(|dir| dir.join(&file))
        .find(|candidate| candidate.is_file())
}

fn locate(name: &str, configured: Option<&Path>, known: &[PathBuf]) -> Option<PathBuf> {
    if let Some(path) = configured.filter(|p| p.is_file()) {
        return Some(path.to_path_buf());
    }
    find_on_path(name).or_else(|| known.iter().find(|p| p.is_file()).cloned())
}

async fn version_of(path: &Path, args: &[&str]) -> Option<String> {
    let spec = ProcessSpec {
        program: path,
        args: args.iter().map(OsString::from).collect(),
        cwd: None,
        stdin: None,
        log_output: false,
    };
    let output = process::run(spec, &NullReporter, &CancellationToken::new()).await.ok()?;
    output.success().then(|| output.stdout.lines().next().unwrap_or_default().trim().to_owned())
}

/// Finds Subversion and checks it is at least 1.10 (needed for `--password-from-stdin`).
pub async fn discover_svn(configured: Option<&Path>) -> ToolReport {
    let install = svn_install_instructions().to_owned();
    let Some(path) = locate("svn", configured, &known_svn_locations()) else {
        return ToolReport {
            kind: ToolKind::Svn,
            path: None,
            version: None,
            ok: false,
            message: "Subversion was not found.".to_owned(),
            fix: Some(install),
        };
    };
    let shown = path.display().to_string();
    let version = version_of(&path, &["--version", "--quiet"]).await;
    let (ok, message) = match version.as_deref().and_then(svn_major_minor) {
        Some(found) if found >= MIN_SVN => {
            (true, format!("Subversion {} at {shown}.", version.as_deref().unwrap_or_default()))
        }
        Some(_) => (
            false,
            format!("Subversion {} is older than 1.10.", version.as_deref().unwrap_or_default()),
        ),
        None => (false, format!("{shown} did not report a version.")),
    };
    ToolReport {
        kind: ToolKind::Svn,
        path: Some(shown),
        version,
        ok,
        fix: (!ok).then_some(install),
        message,
    }
}

/// Finds Git. Git is optional; a missing Git is reported but never blocks.
pub async fn discover_git(configured: Option<&Path>) -> ToolReport {
    let Some(path) = locate("git", configured, &known_git_locations()) else {
        return ToolReport {
            kind: ToolKind::Git,
            path: None,
            version: None,
            ok: false,
            message: "Git was not found. Changelog drafts will compare against trunk instead of your commits.".to_owned(),
            fix: Some(git_install_instructions().to_owned()),
        };
    };
    let shown = path.display().to_string();
    let version = version_of(&path, &["--version"]).await;
    let ok = version.is_some();
    ToolReport {
        kind: ToolKind::Git,
        message: if ok {
            format!("{} at {shown}.", version.as_deref().unwrap_or_default())
        } else {
            format!("{shown} did not report a version.")
        },
        path: Some(shown),
        version,
        ok,
        fix: (!ok).then(|| git_install_instructions().to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_configured_path_falls_back() {
        let missing = Path::new("/definitely/not/here/svn");
        let found = locate("definitely-not-a-real-tool", Some(missing), &[]);
        assert!(found.is_none());
    }

    #[test]
    fn configured_path_wins_when_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join(executable("svn"));
        std::fs::write(&fake, "").unwrap();
        assert_eq!(locate("svn", Some(&fake), &[]), Some(fake));
    }

    #[tokio::test]
    async fn fake_svn_without_version_is_not_ok() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join(executable("svn"));
        std::fs::write(&fake, "not an executable").unwrap();
        let report = discover_svn(Some(&fake)).await;
        assert!(!report.ok);
        assert_eq!(report.path.as_deref(), Some(fake.display().to_string().as_str()));
    }
}
