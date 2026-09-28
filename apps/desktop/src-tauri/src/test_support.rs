//! Test doubles for the shell's unit tests.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use svnpush_core::ai::client::AiClient;
use svnpush_core::project::AppPaths;
use svnpush_core::secret::Secret;
use svnpush_core::vault::{CredentialStore, VaultError};

use crate::events::{EventSink, RunLogEvent, RunStateEvent};
use crate::state::{AppState, RunControl, RunSlot};

/// App state over a temporary folder and an in-memory vault.
pub fn app(dir: &Path) -> AppState {
    AppState::new(AppPaths::new(dir), Arc::new(MemoryVault::default()), AiClient::new().unwrap())
}

/// Puts a run with id `id` in `path`'s slot, as if it were running.
pub fn active_run(app: &AppState, path: &str, id: &str) {
    let (decisions, _) = tokio::sync::mpsc::channel(1);
    let control = RunControl { decisions, cancel: tokio_util::sync::CancellationToken::new() };
    let state = svnpush_core::run::RunState::new(id.to_owned(), path.to_owned(), false);
    app.runs().insert(path.to_owned(), RunSlot { state, control: Some(control) });
}

/// Records the run states sent to the window.
#[derive(Default)]
pub struct RecordingSink(pub Mutex<Vec<RunStateEvent>>);

impl EventSink for RecordingSink {
    fn run_state(&self, event: RunStateEvent) {
        self.0.lock().unwrap().push(event);
    }

    fn run_log(&self, _event: RunLogEvent) {}
}

/// An in-memory credential store.
#[derive(Default)]
pub struct MemoryVault(Mutex<HashMap<String, String>>);

impl CredentialStore for MemoryVault {
    fn get(&self, key: &str) -> Result<Option<Secret>, VaultError> {
        Ok(self.0.lock().unwrap().get(key).map(|v| Secret::new(v.clone())))
    }

    fn set(&self, key: &str, secret: &Secret) -> Result<(), VaultError> {
        self.0.lock().unwrap().insert(key.to_owned(), secret.expose().to_owned());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), VaultError> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}
