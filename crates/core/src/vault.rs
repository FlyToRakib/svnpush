//! The vault: secrets in the operating system keychain, never in a file.
//!
//! Service `svnpush`; SVN passwords under `svn:<host>:<username>`, AI keys
//! under `ai:<provider-id>` (plan §10). The list of SVN accounts (host and
//! username, no password) lives in `accounts.json` because keychains cannot
//! be enumerated portably.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::Coded;
use crate::project::{self, AppPaths, ConfigError, SCHEMA};
use crate::secret::Secret;

/// The keychain service name.
pub const SERVICE: &str = "svnpush";

/// A vault failure.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    /// No usable keychain on this system.
    #[error("the operating system keychain is not available: {detail}")]
    Unavailable { detail: String },
    /// The keychain refused or failed an operation.
    #[error("the keychain could not {action} the secret: {detail}")]
    Backend { action: &'static str, detail: String },
    /// The accounts file could not be read or written.
    #[error(transparent)]
    Config(#[from] ConfigError),
}

impl Coded for VaultError {
    fn code(&self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "VAULT_UNAVAILABLE",
            Self::Backend { .. } => "VAULT_BACKEND",
            Self::Config(inner) => inner.code(),
        }
    }

    fn fix(&self) -> Option<String> {
        match self {
            Self::Unavailable { .. } => Some(
                "Install and unlock a Secret Service keyring (for example GNOME Keyring or KWallet). SVNpush never stores secrets in a file, so publishing and AI stay unavailable until a keychain works.".to_owned(),
            ),
            Self::Backend { .. } => Some("Unlock your keychain and try again.".to_owned()),
            Self::Config(inner) => inner.fix(),
        }
    }
}

/// Reads and writes secrets by key.
pub trait CredentialStore: Send + Sync {
    /// The secret under `key`, or `None` when there is none.
    fn get(&self, key: &str) -> Result<Option<Secret>, VaultError>;
    /// Stores `secret` under `key`, replacing any previous value.
    fn set(&self, key: &str, secret: &Secret) -> Result<(), VaultError>;
    /// Removes the secret under `key`; a missing entry is not an error.
    fn delete(&self, key: &str) -> Result<(), VaultError>;
}

/// The operating system keychain.
#[derive(Debug, Default, Clone, Copy)]
pub struct Keychain;

impl Keychain {
    fn entry(key: &str) -> Result<keyring::Entry, VaultError> {
        if let Err(e) = keyring::Entry::store_status() {
            return Err(VaultError::Unavailable { detail: e.to_string() });
        }
        keyring::Entry::new(SERVICE, key)
            .map_err(|e| VaultError::Backend { action: "open", detail: e.to_string() })
    }

    /// Whether a keychain is usable on this system.
    pub fn status() -> Result<(), VaultError> {
        keyring::Entry::store_status()
            .as_ref()
            .copied()
            .map_err(|e| VaultError::Unavailable { detail: e.to_string() })
    }
}

impl CredentialStore for Keychain {
    fn get(&self, key: &str) -> Result<Option<Secret>, VaultError> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(VaultError::Backend { action: "read", detail: e.to_string() }),
        }
    }

    fn set(&self, key: &str, secret: &Secret) -> Result<(), VaultError> {
        Self::entry(key)?
            .set_password(secret.expose())
            .map_err(|e| VaultError::Backend { action: "store", detail: e.to_string() })
    }

    fn delete(&self, key: &str) -> Result<(), VaultError> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(VaultError::Backend { action: "delete", detail: e.to_string() }),
        }
    }
}

/// The keychain key for an SVN password.
pub fn svn_key(host: &str, username: &str) -> String {
    format!("svn:{host}:{username}")
}

/// The keychain key for an AI provider's API key.
pub fn ai_key(provider_id: &str) -> String {
    format!("ai:{provider_id}")
}

/// The host part of an SVN URL: `plugins.svn.wordpress.org`, or `file` for local repositories.
pub fn svn_host(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty() { scheme.to_owned() } else { host.to_ascii_lowercase() }
}

/// An SVN account known to the vault. The password is in the keychain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SvnAccount {
    /// Server host, for example `plugins.svn.wordpress.org`.
    pub host: String,
    /// Username.
    pub username: String,
}

impl SvnAccount {
    /// This account's keychain key.
    pub fn key(&self) -> String {
        svn_key(&self.host, &self.username)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct AccountsFile {
    schema: u32,
    accounts: Vec<SvnAccount>,
}

/// Every SVN account.
pub fn load_accounts(paths: &AppPaths) -> Result<Vec<SvnAccount>, VaultError> {
    Ok(project::read_json::<AccountsFile>(&paths.accounts_file())?
        .map(|f| f.accounts)
        .unwrap_or_default())
}

/// Saves every SVN account.
pub fn save_accounts(paths: &AppPaths, accounts: &[SvnAccount]) -> Result<(), VaultError> {
    Ok(project::write_json(
        &paths.accounts_file(),
        &AccountsFile { schema: SCHEMA, accounts: accounts.to_vec() },
    )?)
}

/// The account a project uses on `host`: the chosen username, or the only account for that host.
pub fn resolve_account<'a>(
    accounts: &'a [SvnAccount],
    host: &str,
    chosen: Option<&str>,
) -> Option<&'a SvnAccount> {
    let on_host: Vec<&SvnAccount> = accounts.iter().filter(|a| a.host == host).collect();
    match chosen {
        Some(username) => on_host.into_iter().find(|a| a.username == username),
        None if on_host.len() == 1 => on_host.first().copied(),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_hosts() {
        assert_eq!(
            svn_key("plugins.svn.wordpress.org", "bob"),
            "svn:plugins.svn.wordpress.org:bob"
        );
        assert_eq!(ai_key("prov_1"), "ai:prov_1");
        assert_eq!(svn_host("https://Plugins.svn.wordpress.org/demo"), "plugins.svn.wordpress.org");
        assert_eq!(svn_host("file:///C:/repos/demo"), "file");
    }

    #[test]
    fn resolves_the_only_account_or_the_chosen_one() {
        let accounts = vec![
            SvnAccount { host: "a".into(), username: "one".into() },
            SvnAccount { host: "b".into(), username: "two".into() },
            SvnAccount { host: "b".into(), username: "three".into() },
        ];
        assert_eq!(resolve_account(&accounts, "a", None).unwrap().username, "one");
        assert!(resolve_account(&accounts, "b", None).is_none());
        assert_eq!(resolve_account(&accounts, "b", Some("three")).unwrap().username, "three");
        assert!(resolve_account(&accounts, "c", None).is_none());
    }

    #[test]
    fn accounts_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        let accounts = vec![SvnAccount { host: "h".into(), username: "u".into() }];
        save_accounts(&paths, &accounts).unwrap();
        assert_eq!(load_accounts(&paths).unwrap(), accounts);
    }
}
