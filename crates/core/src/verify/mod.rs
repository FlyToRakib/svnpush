//! Step 4, Verify: the gate. Blocking checks V01–V16 and warnings W01–W10.
//!
//! Every check is a pure function of [`VerifyInput`]; gathering the input
//! (listing files, asking `svn`, reading git) happens in the run.

mod blocking;
mod warnings;

use serde::Serialize;
use ts_rs::TS;

use crate::detect::PluginFacts;

pub use blocking::{MIN_SVN, svn_major_minor};

/// Whether a check can stop the release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum Severity {
    /// Must pass for the release to continue.
    Block,
    /// Shown, never blocks.
    Warn,
}

/// A check's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum CheckStatus {
    /// The condition holds.
    Pass,
    /// The condition does not hold.
    Fail,
    /// The check cannot run yet or does not apply; the message says why.
    Skip,
}

/// One row of the check table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CheckResult {
    /// Stable identifier, `V01`–`V16` or `W01`–`W10`.
    pub id: String,
    /// Block or warn.
    pub severity: Severity,
    /// Pass, fail or skip.
    pub status: CheckStatus,
    /// What the check verifies.
    pub title: String,
    /// What was found.
    pub message: String,
    /// What to do about a failure.
    pub fix: Option<String>,
    /// Files involved, relative to the package root.
    pub paths: Vec<String>,
}

/// The state of the sparse SVN working copy, as check V15 sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", content = "paths")]
#[ts(export)]
pub enum WorkingCopyState {
    /// No working copy exists yet; Preview creates it.
    NotCreated,
    /// No conflicts and nothing newer on the server.
    Clean,
    /// Conflicted paths.
    Conflicts(Vec<String>),
    /// Paths changed on the server since the last update.
    OutOfDate(Vec<String>),
}

/// A file as the package checks see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileRef<'a> {
    /// Path relative to the package root, with `/` separators.
    pub rel: &'a str,
    /// Size in bytes.
    pub size: u64,
}

/// Everything the checks read.
#[derive(Debug, Clone, Copy)]
pub struct VerifyInput<'a> {
    /// Detected plugin facts, after Write.
    pub facts: &'a PluginFacts,
    /// The version being released.
    pub version: &'a str,
    /// The previous release, when there is one.
    pub previous: Option<&'a str>,
    /// Folder names under `tags/` on the server.
    pub server_tags: &'a [String],
    /// Raw `readme.txt`, when present.
    pub readme_text: Option<&'a str>,
    /// Raw main plugin file.
    pub main_file_text: &'a str,
    /// Files that will be (or were) packaged.
    pub files: &'a [FileRef<'a>],
    /// Entry names of the built zip; `None` before Build.
    pub zip_entries: Option<&'a [String]>,
    /// Extra paths the project marks required.
    pub required_paths: &'a [String],
    /// Whether the project allows `.phar` files.
    pub allow_phar: bool,
    /// `svn --version --quiet` output; `None` when not found.
    pub svn_version: Option<&'a str>,
    /// Working copy state.
    pub working_copy: &'a WorkingCopyState,
    /// Whether the vault holds credentials for the project's SVN account.
    pub has_credentials: bool,
    /// Uncommitted git paths; `None` when git or a repository is unavailable.
    pub git_dirty: Option<&'a [String]>,
    /// The current WordPress version; `None` when the lookup is off or failed.
    pub current_wordpress: Option<&'a str>,
    /// File names in the assets folder; `None` when there is no assets folder.
    pub assets: Option<&'a [String]>,
    /// Packaged files ignored by a `.gitignore`.
    pub gitignored: &'a [String],
}

impl CheckResult {
    pub(crate) fn new(id: &str, severity: Severity, title: &str) -> Self {
        Self {
            id: id.to_owned(),
            severity,
            status: CheckStatus::Pass,
            title: title.to_owned(),
            message: String::new(),
            fix: None,
            paths: Vec::new(),
        }
    }

    pub(crate) fn pass(mut self, message: impl Into<String>) -> Self {
        self.status = CheckStatus::Pass;
        self.message = message.into();
        self
    }

    pub(crate) fn fail(mut self, message: impl Into<String>, fix: impl Into<String>) -> Self {
        self.status = CheckStatus::Fail;
        self.message = message.into();
        let fix = fix.into();
        self.fix = (!fix.is_empty()).then_some(fix);
        self
    }

    pub(crate) fn skip(mut self, message: impl Into<String>) -> Self {
        self.status = CheckStatus::Skip;
        self.message = message.into();
        self
    }

    pub(crate) fn with_paths(mut self, paths: Vec<String>) -> Self {
        self.paths = paths;
        self
    }
}

/// Runs every check, blocking first, in ID order.
pub fn run(input: &VerifyInput<'_>) -> Vec<CheckResult> {
    let mut results = blocking::all(input);
    results.extend(warnings::all(input));
    results
}

/// Runs only V10–V13, the checks Build repeats against the staged tree and zip.
pub fn package_checks(input: &VerifyInput<'_>) -> Vec<CheckResult> {
    blocking::package(input)
}

/// Whether any blocking check failed.
pub fn is_blocked(results: &[CheckResult]) -> bool {
    results.iter().any(|r| r.severity == Severity::Block && r.status == CheckStatus::Fail)
}

/// The IDs of every failed check, blocking and warning, for the journal.
pub fn failed_ids(results: &[CheckResult]) -> Vec<String> {
    results.iter().filter(|r| r.status == CheckStatus::Fail).map(|r| r.id.clone()).collect()
}
