//! The state the shell owns for the lifetime of the app.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use svnpush_core::ai::client::AiClient;
use svnpush_core::project::AppPaths;
use svnpush_core::run::{Decision, RunState};
use svnpush_core::vault::CredentialStore;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// How the UI talks to a run that is still going.
#[derive(Debug, Clone)]
pub struct RunControl {
    /// Approve and Publish answers.
    pub decisions: mpsc::Sender<Decision>,
    /// Cancel.
    pub cancel: CancellationToken,
}

/// The latest state of a project's run, and its controls while it runs.
#[derive(Debug, Clone)]
pub struct RunSlot {
    /// The most recent state.
    pub state: RunState,
    /// `None` once the run has ended.
    pub control: Option<RunControl>,
}

/// App-wide state, shared by every command.
pub struct AppState {
    /// App data folders.
    pub paths: AppPaths,
    /// The keychain.
    pub vault: Arc<dyn CredentialStore>,
    /// The only AI HTTP client.
    pub ai: AiClient,
    /// Serialises reads and writes of `projects.json`.
    pub projects_lock: tokio::sync::Mutex<()>,
    runs: Mutex<HashMap<String, RunSlot>>,
}

impl AppState {
    /// State over `paths`, `vault` and the AI client.
    pub fn new(paths: AppPaths, vault: Arc<dyn CredentialStore>, ai: AiClient) -> Self {
        Self {
            paths,
            vault,
            ai,
            projects_lock: tokio::sync::Mutex::new(()),
            runs: Mutex::new(HashMap::new()),
        }
    }

    /// The run slots. A poisoned lock is recovered: the map holds plain data.
    pub fn runs(&self) -> MutexGuard<'_, HashMap<String, RunSlot>> {
        self.runs.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether this window has a release starting or running for `path`.
    pub fn is_active(&self, path: &str) -> bool {
        self.runs().get(path).is_some_and(|slot| slot.control.is_some())
    }

    /// Whether this window has any release starting or running.
    pub fn any_active(&self) -> bool {
        self.runs().values().any(|slot| slot.control.is_some())
    }
}
