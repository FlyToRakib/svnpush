//! The run's shape: steps, phases, the draft, and the one `RunState` the UI renders.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::detect::{GitFacts, PluginFacts};
use crate::error::Coded;
use crate::package::Package;
use crate::svn::{Delta, TagVerification};
use crate::verify::CheckResult;

use super::ai_view::{DraftAi, Explanation};
use super::changes::ChangeSet;

/// The seven protocol steps, in order (plan §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Step {
    /// 1. Detect.
    Detect,
    /// 2. Changes and draft.
    Draft,
    /// 3. Write.
    Write,
    /// 4. Verify.
    Verify,
    /// 5. Build.
    Build,
    /// 6. Preview SVN.
    Preview,
    /// 7. Publish.
    Publish,
}

impl Step {
    /// Every step in order.
    pub const ALL: [Self; 7] = [
        Self::Detect,
        Self::Draft,
        Self::Write,
        Self::Verify,
        Self::Build,
        Self::Preview,
        Self::Publish,
    ];

    /// The 1-based number shown in the checklist.
    pub fn number(self) -> u8 {
        match self {
            Self::Detect => 1,
            Self::Draft => 2,
            Self::Write => 3,
            Self::Verify => 4,
            Self::Build => 5,
            Self::Preview => 6,
            Self::Publish => 7,
        }
    }
}

/// A step card's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum StepStatus {
    /// Not reached.
    Pending,
    /// Working.
    Running,
    /// Waiting for the developer.
    Waiting,
    /// Finished.
    Done,
    /// Stopped the run.
    Failed,
    /// Not part of this run (Publish in a dry run).
    Skipped,
}

/// One card in the checklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct StepView {
    /// Which step.
    pub step: Step,
    /// Its state.
    pub status: StepStatus,
    /// One line about what happened.
    pub summary: Option<String>,
}

/// Where the run is (plan §13 state machine).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Phase {
    /// Created, not started.
    Idle,
    /// Step 1.
    Detecting,
    /// Step 2, computing changes.
    Drafting,
    /// Step 2, waiting for Approve.
    AwaitingApproval,
    /// Step 3.
    Writing,
    /// Step 4.
    Verifying,
    /// Step 4 failed; waiting for suggested fixes to be applied or the run stopped.
    AwaitingFixes,
    /// Step 5.
    Building,
    /// Step 6.
    Previewing,
    /// Step 7, waiting for the Publish confirmation.
    AwaitingPublish,
    /// Step 7, committing and tagging.
    Publishing,
    /// Published and the tag verified.
    Verified,
    /// Published; the tag could not be verified yet.
    PublishedUnverified,
    /// A dry run finished its preview.
    DryRunComplete,
    /// A step failed.
    Failed,
    /// The developer cancelled.
    Cancelled,
}

impl Phase {
    /// Whether the run has ended.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Verified
                | Self::PublishedUnverified
                | Self::DryRunComplete
                | Self::Failed
                | Self::Cancelled
        )
    }
}

/// A typed error, ready to render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ErrorView {
    /// Stable code.
    pub code: String,
    /// What went wrong.
    pub message: String,
    /// What to do about it.
    pub fix: Option<String>,
}

impl ErrorView {
    /// From any coded error.
    pub fn from_coded(error: &dyn Coded) -> Self {
        Self {
            code: error.code().to_owned(),
            message: capitalise(&error.to_string()),
            fix: error.fix(),
        }
    }

    /// A run-level error with a fixed code.
    pub fn new(code: &str, message: impl Into<String>, fix: Option<String>) -> Self {
        Self { code: code.to_owned(), message: message.into(), fix }
    }
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// The AI provider that wrote a draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DraftProvider {
    /// Provider record id.
    pub id: String,
    /// Adapter kind.
    pub kind: String,
    /// Record label.
    pub label: String,
}

/// The release text the developer approves (plan §13 `ReleaseDraft`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReleaseDraft {
    /// The version to release.
    pub version: String,
    /// Why this version (one sentence).
    pub reason: String,
    /// The changelog entry body.
    pub changelog_markdown: String,
    /// The upgrade notice; empty for none.
    pub upgrade_notice: String,
    /// One paragraph about the release.
    pub summary: String,
    /// Who wrote it; `None` when written by hand.
    pub provider: Option<DraftProvider>,
    /// The provider that failed before this one answered.
    pub fell_back_from: Option<String>,
}

/// What Step 2 knows before a draft exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct DraftContext {
    /// The previous release, if any.
    pub previous: Option<String>,
    /// The next patch version, pre-filled in the form.
    pub suggested_version: String,
    /// What changed.
    pub changes: ChangeSet,
    /// A pre-filled draft from the readme and commits.
    pub prefill: ReleaseDraft,
}

/// A unified diff of one written file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FileDiff {
    /// Path relative to the project folder.
    pub path: String,
    /// Unified diff text.
    pub diff: String,
}

/// What Step 6 will commit and tag (plan §13 `SvnPreview`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SvnPreview {
    /// Changes under `trunk/`.
    pub trunk: Delta,
    /// Changes under `assets/`.
    pub assets: Delta,
    /// Default trunk commit message.
    pub trunk_message: String,
    /// Default tag commit message.
    pub tag_message: String,
    /// The tag URL that will be created.
    pub tag_url: String,
    /// The SVN URL.
    pub svn_url: String,
    /// The account that will commit, when one is set up.
    pub account: Option<String>,
    /// Per-file diffs, keyed `trunk/<path>` or `assets/<path>`, captured before
    /// a dry run reverts the working copy. Large diffs are cut short.
    pub diffs: Vec<FileDiff>,
}

/// What Step 7 did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PublishResult {
    /// Trunk commit revision; `None` when trunk was unchanged.
    #[ts(type = "number | null")]
    pub trunk_revision: Option<u64>,
    /// Tag commit revision.
    #[ts(type = "number | null")]
    pub tag_revision: Option<u64>,
    /// Tag verification outcome.
    pub verification: Option<TagVerification>,
    /// The public plugin page.
    pub plugin_url: String,
    /// Whether the plugin page should open (project setting).
    pub open_plugin_page: bool,
    /// The zip that was published.
    pub zip_path: Option<String>,
}

/// The developer's answers while a run waits.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind")]
#[ts(export)]
pub enum Decision {
    /// Step 2: write this draft.
    Approve {
        /// The approved draft.
        draft: ReleaseDraft,
    },
    /// Step 2 or 4: ask the AI, optionally with another provider (the Change link).
    Generate {
        /// The provider record; `None` resolves it as usual.
        provider_id: Option<String>,
    },
    /// Step 2 or 4: the data-sharing notice for this provider was accepted.
    AcceptPrivacy {
        /// The provider record.
        provider_id: String,
    },
    /// Step 2: stop the AI and write the draft by hand.
    Manual,
    /// Step 4: apply these suggested readme fixes (indexes into the explanation) and verify again.
    ApplyFixes {
        /// Indexes into `Explanation::fixes`.
        fixes: Vec<u32>,
    },
    /// Step 4: stop with the failed checks.
    Stop,
    /// Step 7: commit and tag with these messages.
    Publish {
        /// Trunk commit message.
        trunk_message: String,
        /// Tag commit message.
        tag_message: String,
    },
}

/// Everything the UI renders about one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RunState {
    /// Run id (a sortable timestamp).
    pub id: String,
    /// Project folder.
    pub project_path: String,
    /// Whether this is a dry run.
    pub dry_run: bool,
    /// Where the run is.
    pub phase: Phase,
    /// The seven step cards.
    pub steps: Vec<StepView>,
    /// Step 1 result.
    pub facts: Option<PluginFacts>,
    /// Git facts, when git is available.
    pub git: Option<GitFacts>,
    /// Tags on the server.
    pub server_tags: Vec<String>,
    /// The previous release: the newest of the trunk working copy and the server tags.
    pub previous: Option<String>,
    /// Step 2 context.
    pub draft_context: Option<DraftContext>,
    /// The approved draft.
    pub draft: Option<ReleaseDraft>,
    /// Step 3 diffs.
    pub diffs: Vec<FileDiff>,
    /// Step 2's AI panel.
    pub draft_ai: Option<DraftAi>,
    /// Step 4's explanation of failed checks.
    pub explanation: Option<Explanation>,
    /// Step 4 (and Build's V10–V13) results.
    pub checks: Vec<CheckResult>,
    /// Step 5 result.
    pub package: Option<Package>,
    /// Step 6 result.
    pub preview: Option<SvnPreview>,
    /// Step 7 result.
    pub publish: Option<PublishResult>,
    /// The error that stopped the run.
    pub error: Option<ErrorView>,
    /// Informational notes (long paths, offline tag list, overrides).
    pub notices: Vec<String>,
}

impl RunState {
    /// A fresh state with every step pending.
    pub fn new(id: String, project_path: String, dry_run: bool) -> Self {
        Self {
            id,
            project_path,
            dry_run,
            phase: Phase::Idle,
            steps: Step::ALL
                .iter()
                .map(|&step| StepView { step, status: StepStatus::Pending, summary: None })
                .collect(),
            facts: None,
            git: None,
            server_tags: Vec::new(),
            previous: None,
            draft_context: None,
            draft: None,
            diffs: Vec::new(),
            draft_ai: None,
            explanation: None,
            checks: Vec::new(),
            package: None,
            preview: None,
            publish: None,
            error: None,
            notices: Vec::new(),
        }
    }

    /// Sets a step's status and summary.
    pub fn set_step(&mut self, step: Step, status: StepStatus, summary: Option<String>) {
        if let Some(view) = self.steps.iter_mut().find(|v| v.step == step) {
            view.status = status;
            if summary.is_some() {
                view.summary = summary;
            }
        }
    }

    /// The step currently running or waiting, if any.
    pub fn active_step(&self) -> Option<Step> {
        self.steps
            .iter()
            .find(|v| matches!(v.status, StepStatus::Running | StepStatus::Waiting))
            .map(|v| v.step)
    }
}
