//! Test doubles for the shell's unit tests.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use svnpush_core::ai::client::AiClient;
use svnpush_core::project::AppPaths;
use svnpush_core::secret::Secret;
use svnpush_core::vault::{CredentialStore, VaultError};

use crate::state::AppState;

/// App state over a temporary folder and an in-memory vault.
pub fn app(dir: &Path) -> AppState {
    AppState::new(AppPaths::new(dir), Arc::new(MemoryVault::default()), AiClient::new().unwrap())
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
