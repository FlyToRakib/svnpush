//! The release run: the seven-step state machine, its journal, rollback,
//! lock, cancel and resume (plan §5, §12, §13).

mod ai_view;
mod assets;
mod assist;
pub mod changes;
mod draft;
mod engine;
mod explain;
mod hook;
pub mod journal;
mod lock;
pub mod material;
pub mod model;
mod publish;
mod snapshot;
mod steps;
mod svn_steps;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::ai::client::AiClient;
use crate::error::Coded;
use crate::project::{AppPaths, Project};
use crate::report::Reporter;
use crate::tools::ToolReport;
use crate::vault::{CredentialStore, SvnAccount};

pub use ai_view::{
    AiStatus, AiTask, DraftAi, Explanation, FixView, PrivacyNotice, SummaryProgress,
};
pub use draft::{prefill, suggested_version, validate_draft};
pub use engine::Run;
pub use journal::{Outcome, RunJournal};
pub use lock::ProjectLock;
pub use model::{Decision, ErrorView, Phase, ReleaseDraft, RunState, Step, StepStatus};
pub use publish::{discard, resume_tag};
pub use snapshot::Snapshot;

/// Receives the run's state after every change, and its log lines.
pub trait RunObserver: Reporter {
    /// The complete new state.
    fn state(&self, state: &RunState);
}

/// Everything a run needs from the outside world.
pub struct RunInputs {
    /// App data folders.
    pub paths: AppPaths,
    /// The project, with `.svnpush.json` applied.
    pub project: Project,
    /// Whether to stop after the SVN preview.
    pub dry_run: bool,
    /// Whether to sync and commit only `assets/` (plan §8.7).
    pub assets_only: bool,
    /// The Subversion discovery result.
    pub svn: ToolReport,
    /// Git, when found.
    pub git: Option<PathBuf>,
    /// The keychain.
    pub vault: Arc<dyn CredentialStore>,
    /// Known SVN accounts.
    pub accounts: Vec<SvnAccount>,
    /// The current WordPress version, when the lookup is on and succeeded.
    pub current_wordpress: Option<String>,
    /// The AI client. Provider records and the privacy notices seen are read
    /// from disk when needed, so changes made during the run apply.
    pub ai: AiClient,
}

/// Why a run stopped early.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFailure {
    /// The error to show.
    pub error: ErrorView,
    /// Whether the developer cancelled.
    pub cancelled: bool,
}

impl RunFailure {
    /// A failure with a fixed code.
    pub fn new(code: &str, message: impl Into<String>, fix: Option<String>) -> Self {
        Self { error: ErrorView::new(code, message, fix), cancelled: false }
    }

    /// The developer cancelled.
    pub fn cancelled() -> Self {
        Self {
            error: ErrorView::new("CANCELLED", "The release was cancelled.", None),
            cancelled: true,
        }
    }

    /// A filesystem failure.
    pub fn io(action: &str, path: &Path, source: &std::io::Error) -> Self {
        Self::new("RUN_IO", format!("Could not {action} {}: {source}", path.display()), None)
    }
}

impl<E: Coded> From<E> for RunFailure {
    fn from(error: E) -> Self {
        if error.code() == "CANCELLED" {
            Self::cancelled()
        } else {
            Self { error: ErrorView::from_coded(&error), cancelled: false }
        }
    }
}
