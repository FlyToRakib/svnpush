//! Test doubles for the shell's unit tests.

use std::collections::HashMap;
use std::sync::Mutex;

use svnpush_core::secret::Secret;
use svnpush_core::vault::{CredentialStore, VaultError};

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
