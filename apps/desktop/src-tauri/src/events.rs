//! Events from the core to the UI: run state snapshots and log lines.

use std::sync::Arc;

use serde::Serialize;
use svnpush_core::report::{LogLine, Reporter};
use svnpush_core::run::{RunObserver, RunState};
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

use crate::state::AppState;

/// The event carrying a run's full state.
pub const RUN_STATE_EVENT: &str = "run-state";
/// The event carrying one log line.
pub const RUN_LOG_EVENT: &str = "run-log";

/// Payload of [`RUN_STATE_EVENT`].
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RunStateEvent {
    /// The project the run belongs to.
    pub project_path: String,
    /// The state.
    pub state: RunState,
}

/// Payload of [`RUN_LOG_EVENT`].
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RunLogEvent {
    /// The project the run belongs to.
    pub project_path: String,
    /// The line.
    pub line: LogLine,
}

/// Where run events go: the window in the app, a recorder in tests.
pub trait EventSink: Send + Sync {
    /// A new run state.
    fn run_state(&self, event: RunStateEvent);
    /// A log line.
    fn run_log(&self, event: RunLogEvent);
}

/// Emits events to every window.
pub struct TauriSink(pub AppHandle);

impl EventSink for TauriSink {
    fn run_state(&self, event: RunStateEvent) {
        let _ = self.0.emit(RUN_STATE_EVENT, event);
    }

    fn run_log(&self, event: RunLogEvent) {
        let _ = self.0.emit(RUN_LOG_EVENT, event);
    }
}

/// Observes one project's run: keeps the latest state in [`AppState`] and
/// forwards everything to the sink unchanged.
pub struct ProjectObserver {
    /// The project folder.
    pub project_path: String,
    /// Where the slot lives.
    pub app: Arc<AppState>,
    /// Where events go.
    pub sink: Arc<dyn EventSink>,
}

impl Reporter for ProjectObserver {
    fn log(&self, line: LogLine) {
        self.sink.run_log(RunLogEvent { project_path: self.project_path.clone(), line });
    }
}

impl RunObserver for ProjectObserver {
    fn state(&self, state: &RunState) {
        if let Some(slot) = self.app.runs().get_mut(&self.project_path) {
            slot.state = state.clone();
        }
        self.sink.run_state(RunStateEvent {
            project_path: self.project_path.clone(),
            state: state.clone(),
        });
    }
}
