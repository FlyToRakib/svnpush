//! What the UI shows about the AI inside a run: the draft panel in Step 2 and
//! the explanation of failed checks in Step 4 (plan §5.2, §5.4, §9.8, §12.4).

use serde::Serialize;
use ts_rs::TS;

use crate::ai::provider::Fleet;
use crate::ai::records::ProviderRecord;

use super::model::{DraftProvider, ErrorView, ReleaseDraft};

impl From<&ProviderRecord> for DraftProvider {
    fn from(record: &ProviderRecord) -> Self {
        Self { id: record.id.clone(), kind: record.kind.clone(), label: record.label.clone() }
    }
}

/// Where an AI task is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum AiStatus {
    /// The project turned AI off.
    Off,
    /// No provider could be resolved.
    NoProvider,
    /// Waiting for the developer to accept the data-sharing notice.
    NeedsConsent,
    /// Asking the provider.
    Working,
    /// The answer arrived.
    Done,
    /// The provider failed; the error says why.
    Failed,
    /// The developer chose to write it by hand.
    Manual,
}

/// The one-time notice shown before a provider first receives project data.
/// The UI lists what leaves the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PrivacyNotice {
    /// Who will receive the data.
    pub provider: DraftProvider,
    /// Whether the provider is Revoye (answers come from the developer's own AI accounts).
    pub revoye: bool,
}

/// Per-file summaries written so far, when the diff was too large.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SummaryProgress {
    /// Files summarised.
    pub done: u32,
    /// Files to summarise.
    pub total: u32,
}

/// Progress and outcome of one AI task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AiTask {
    /// Where it is.
    pub status: AiStatus,
    /// The provider asked (the last one, after a fallback).
    pub provider: Option<DraftProvider>,
    /// Every configured provider, for the Change link.
    pub choices: Vec<DraftProvider>,
    /// Summaries written so far, when the diff was too large.
    pub summaries: Option<SummaryProgress>,
    /// Revoye job status while pending.
    pub job_status: Option<String>,
    /// Revoye queue position while queued.
    pub queue_position: Option<u32>,
    /// Revoye fleet snapshot, refreshed once a minute while waiting.
    pub fleet: Option<Fleet>,
    /// Why it failed.
    pub error: Option<ErrorView>,
    /// The unusable answer, shown as plain text.
    pub raw_text: Option<String>,
    /// The notice to accept first.
    pub privacy: Option<PrivacyNotice>,
}

impl AiTask {
    /// A task in `status` with no progress.
    pub fn new(status: AiStatus, choices: Vec<DraftProvider>) -> Self {
        Self {
            status,
            provider: None,
            choices,
            summaries: None,
            job_status: None,
            queue_position: None,
            fleet: None,
            error: None,
            raw_text: None,
            privacy: None,
        }
    }

    /// Clears progress and outcome before a new attempt.
    pub fn start(&mut self, provider: DraftProvider) {
        self.status = AiStatus::Working;
        self.provider = Some(provider);
        self.summaries = None;
        self.job_status = None;
        self.queue_position = None;
        self.error = None;
        self.raw_text = None;
        self.privacy = None;
    }
}

/// Step 2's AI panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct DraftAi {
    /// The task.
    pub task: AiTask,
    /// The AI's draft, loaded into the editor when `generation` changes.
    pub draft: Option<ReleaseDraft>,
    /// Increments with each new AI draft.
    pub generation: u32,
    /// Whether the draft was written from per-file summaries.
    pub from_summaries: bool,
    /// A note about the draft, for example an overridden version.
    pub notice: Option<String>,
}

/// One suggested readme edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FixView {
    /// The check it fixes.
    pub check_id: String,
    /// The file (only `readme.txt` can be applied).
    pub path: String,
    /// Text to replace.
    pub original: String,
    /// Replacement text.
    pub replacement: String,
    /// The diff applying it would produce.
    pub diff: String,
    /// Why it cannot be applied, when it cannot.
    pub problem: Option<String>,
}

/// Step 4's explanation of failed checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Explanation {
    /// The task.
    pub task: AiTask,
    /// Plain-language explanation.
    pub text: Option<String>,
    /// Suggested readme edits.
    pub fixes: Vec<FixView>,
}
