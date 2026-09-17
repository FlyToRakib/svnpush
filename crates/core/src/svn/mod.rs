//! The SVN engine: a sparse working copy, trunk and assets sync, commit,
//! server-side tag and post-publish verification (plan §8).

mod remote;
mod status;
mod sync;
mod wc;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::error::Coded;
use crate::report::Reporter;
use crate::secret::Secret;
use crate::tools::process::{self, ProcessError, ProcessOutput, ProcessSpec};

pub use remote::{TagVerification, VERIFY_ATTEMPTS, VERIFY_DELAY};
pub use status::{StatusEntry, StatusItem};
pub use sync::{SourceFile, folder_files, mime_type, source_files};
pub use wc::DEEP_FOLDERS;

/// Seconds `svn` waits on a silent network connection before giving up (plan §12.2).
pub const NETWORK_TIMEOUT_SECONDS: u32 = 60;

/// Paths passed to one `svn` invocation, keeping command lines well under OS limits.
const TARGETS_PER_CALL: usize = 100;

/// An SVN account's credentials. The password travels only over stdin.
#[derive(Debug)]
pub struct Credentials {
    /// The WordPress.org username.
    pub username: String,
    /// The SVN password (the application password from the WordPress.org profile).
    pub password: Secret,
}

/// What a sync would add, change and delete (plan §13 `Delta`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Delta {
    /// New paths, relative to the synced folder.
    pub added: Vec<String>,
    /// Changed files.
    pub modified: Vec<String>,
    /// Removed paths; folders end with `/`.
    pub deleted: Vec<String>,
}

impl Delta {
    /// Whether nothing changes.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }
}

/// An SVN failure.
#[derive(Debug, thiserror::Error)]
pub enum SvnError {
    /// The server rejected the username or password (401).
    #[error("the SVN server rejected the credentials for {username}")]
    CredentialsRejected { username: String },
    /// The account may not commit to this plugin (403).
    #[error("{username} is not a committer for this plugin")]
    Forbidden { username: String },
    /// The server could not be reached.
    #[error("could not reach the SVN server: {detail}")]
    Network { detail: String },
    /// Something changed on the server since the last update.
    #[error("the working copy is out of date: {detail}")]
    OutOfDate { detail: String },
    /// The working copy is locked or conflicted.
    #[error("the working copy needs attention: {detail}")]
    WorkingCopy { detail: String },
    /// A URL does not exist on the server.
    #[error("not found on the server: {detail}")]
    NotFound { detail: String },
    /// `tags/<version>` already exists.
    #[error("tags/{version} already exists on the server")]
    TagExists { version: String },
    /// Any other failed `svn` invocation.
    #[error("svn {command} failed: {detail}")]
    Failed { command: String, detail: String },
    /// The operation was cancelled.
    #[error("cancelled")]
    Cancelled,
    /// `svn` could not be run.
    #[error(transparent)]
    Process(ProcessError),
    /// A local filesystem operation failed.
    #[error("{action} {path}: {source}")]
    Io { action: &'static str, path: String, source: std::io::Error },
    /// Output could not be understood.
    #[error("could not read svn output: {detail}")]
    Parse { detail: String },
}

impl Coded for SvnError {
    fn code(&self) -> &'static str {
        match self {
            Self::CredentialsRejected { .. } => "SVN_CREDENTIALS_REJECTED",
            Self::Forbidden { .. } => "SVN_FORBIDDEN",
            Self::Network { .. } => "SVN_NETWORK",
            Self::OutOfDate { .. } => "SVN_OUT_OF_DATE",
            Self::WorkingCopy { .. } => "SVN_WORKING_COPY",
            Self::NotFound { .. } => "SVN_NOT_FOUND",
            Self::TagExists { .. } => "SVN_TAG_EXISTS",
            Self::Failed { .. } => "SVN_FAILED",
            Self::Cancelled => "CANCELLED",
            Self::Process(_) => "SVN_NOT_RUNNABLE",
            Self::Io { .. } => "SVN_IO",
            Self::Parse { .. } => "SVN_PARSE",
        }
    }

    fn fix(&self) -> Option<String> {
        match self {
            Self::CredentialsRejected { .. } => Some(
                "Use the SVN password from your WordPress.org profile (Account & Security), not your account password, and update it in Vault.".to_owned(),
            ),
            Self::Forbidden { username } => Some(format!(
                "Ask a plugin owner to add {username} as a committer on the plugin's Advanced View page."
            )),
            Self::Network { .. } => Some("Check your connection and try again.".to_owned()),
            Self::OutOfDate { .. } | Self::WorkingCopy { .. } => {
                Some("Reset the working copy and run the release again.".to_owned())
            }
            Self::NotFound { .. } => Some("Check the project's SVN URL.".to_owned()),
            Self::TagExists { .. } => Some("Release a new version number.".to_owned()),
            Self::Process(_) => Some(crate::tools::svn_install_instructions().to_owned()),
            Self::Failed { .. } | Self::Cancelled | Self::Io { .. } | Self::Parse { .. } => None,
        }
    }
}

pub(crate) fn io_error(action: &'static str, path: &Path, source: std::io::Error) -> SvnError {
    SvnError::Io { action, path: path.display().to_string(), source }
}

/// Converts a local path to a `/`-separated string for display and relative keys.
pub(crate) fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Runs `svn` with one reporter and one cancellation token.
pub struct Svn<'a> {
    bin: PathBuf,
    reporter: &'a dyn Reporter,
    cancel: CancellationToken,
}

impl<'a> Svn<'a> {
    /// A client for the `svn` binary at `bin`.
    pub fn new(
        bin: impl Into<PathBuf>,
        reporter: &'a dyn Reporter,
        cancel: CancellationToken,
    ) -> Self {
        Self { bin: bin.into(), reporter, cancel }
    }

    /// Runs `svn <args>` locally (no server contact).
    async fn local(&self, args: Vec<OsString>) -> Result<ProcessOutput, SvnError> {
        self.exec(args, None, true).await
    }

    /// Runs `svn <args>` against the server, with credentials over stdin when given.
    async fn network(
        &self,
        mut args: Vec<OsString>,
        credentials: Option<&Credentials>,
        log_output: bool,
    ) -> Result<ProcessOutput, SvnError> {
        args.push("--config-option".into());
        args.push(format!("servers:global:http-timeout={NETWORK_TIMEOUT_SECONDS}").into());
        args.push("--no-auth-cache".into());
        if let Some(c) = credentials {
            args.push("--username".into());
            args.push(c.username.clone().into());
            args.push("--password-from-stdin".into());
        }
        self.exec(args, credentials, log_output).await
    }

    async fn exec(
        &self,
        mut args: Vec<OsString>,
        credentials: Option<&Credentials>,
        log_output: bool,
    ) -> Result<ProcessOutput, SvnError> {
        let command = args.first().map(|a| a.to_string_lossy().into_owned()).unwrap_or_default();
        args.push("--non-interactive".into());
        let spec = ProcessSpec {
            program: &self.bin,
            args,
            cwd: None,
            stdin: credentials.map(|c| &c.password),
            log_output,
        };
        let output =
            process::run(spec, self.reporter, &self.cancel).await.map_err(|e| match e {
                ProcessError::Cancelled => SvnError::Cancelled,
                other => SvnError::Process(other),
            })?;
        if output.success() {
            Ok(output)
        } else {
            let username = credentials.map(|c| c.username.clone()).unwrap_or_default();
            Err(classify(&command, &output.stderr, username))
        }
    }

    /// Runs `svn <command> <fixed args> <paths>` in batches.
    async fn batched(&self, command: &[&str], paths: &[PathBuf]) -> Result<(), SvnError> {
        for chunk in paths.chunks(TARGETS_PER_CALL) {
            let mut args: Vec<OsString> = command.iter().map(OsString::from).collect();
            args.extend(chunk.iter().map(|p| p.as_os_str().to_owned()));
            self.local(args).await?;
        }
        Ok(())
    }
}

/// Maps `svn` error codes (locale-independent) to typed errors.
fn classify(command: &str, stderr: &str, username: String) -> SvnError {
    let detail = stderr
        .lines()
        .filter(|l| l.contains(": E") || l.starts_with("svn:"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ");
    let detail = if detail.is_empty() { stderr.trim().to_owned() } else { detail };
    let has = |codes: &[&str]| codes.iter().any(|c| stderr.contains(c));

    if has(&["E170001", "E215004"]) {
        SvnError::CredentialsRejected { username }
    } else if has(&["E175013", "E220004", "E170011"]) || stderr.contains("403 Forbidden") {
        SvnError::Forbidden { username }
    } else if has(&["E155011", "E160028", "E160024", "E170004", "E155035"]) {
        SvnError::OutOfDate { detail }
    } else if has(&["E155004", "E155037", "E155015", "E155007", "E155016"]) {
        SvnError::WorkingCopy { detail }
    } else if has(&["E160013", "E170000", "E200009", "E180001"]) {
        SvnError::NotFound { detail }
    } else if has(&[
        "E170013", "E175002", "E670002", "E670008", "E731001", "E730", "E120", "E175012",
    ]) {
        SvnError::Network { detail }
    } else {
        SvnError::Failed { command: command.to_owned(), detail }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_error_code() {
        let e = classify("commit", "svn: E170001: Authorization failed\n", "bob".into());
        assert_eq!(e.code(), "SVN_CREDENTIALS_REJECTED");
        assert!(e.fix().unwrap().contains("WordPress.org profile"));

        let e = classify("commit", "svn: E175013: Access to '/x' forbidden\n", "bob".into());
        assert_eq!(e.code(), "SVN_FORBIDDEN");
        assert!(e.fix().unwrap().contains("bob"));

        let e = classify("commit", "svn: E155011: File 'a' is out of date\n", String::new());
        assert_eq!(e.code(), "SVN_OUT_OF_DATE");

        let e = classify(
            "ls",
            "svn: E170013: Unable to connect\nsvn: E731001: No such host is known.\n",
            String::new(),
        );
        assert_eq!(e.code(), "SVN_NETWORK");

        let e = classify(
            "ls",
            "svn: warning: W160013: path not found\nsvn: E200009: Could not list\n",
            String::new(),
        );
        assert_eq!(e.code(), "SVN_NOT_FOUND");

        let e = classify("update", "svn: E155004: Working copy locked\n", String::new());
        assert_eq!(e.code(), "SVN_WORKING_COPY");

        let e = classify("x", "svn: E999999: odd\n", String::new());
        assert_eq!(e.code(), "SVN_FAILED");
    }
}
