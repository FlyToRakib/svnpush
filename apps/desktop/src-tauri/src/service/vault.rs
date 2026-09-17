//! The Vault screen: SVN accounts with passwords in the keychain.

use std::path::PathBuf;

use serde::Serialize;
use svnpush_core::report::NullReporter;
use svnpush_core::run::ErrorView;
use svnpush_core::secret::Secret;
use svnpush_core::svn::{Credentials, Svn};
use svnpush_core::vault::{self, Keychain, SvnAccount};
use svnpush_core::{settings, tools};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::state::AppState;

/// An account as the Vault screen shows it. The password is never returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AccountView {
    /// Server host.
    pub host: String,
    /// Username.
    pub username: String,
    /// Whether the keychain holds a password.
    pub has_password: bool,
}

/// Accounts and whether the keychain works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct VaultView {
    /// Every account.
    pub accounts: Vec<AccountView>,
    /// Why the keychain cannot be used, when it cannot.
    pub keychain_problem: Option<ErrorView>,
}

fn coded(e: &impl svnpush_core::Coded) -> ErrorView {
    ErrorView::from_coded(e)
}

/// Every account.
pub fn view(app: &AppState) -> Result<VaultView, ErrorView> {
    let accounts = vault::load_accounts(&app.paths).map_err(|e| coded(&e))?;
    Ok(VaultView {
        accounts: accounts
            .into_iter()
            .map(|a| AccountView {
                has_password: matches!(app.vault.get(&a.key()), Ok(Some(_))),
                host: a.host,
                username: a.username,
            })
            .collect(),
        keychain_problem: Keychain::status().err().map(|e| coded(&e)),
    })
}

/// Adds or updates an account. A new account needs a password; an existing
/// one keeps its password when `password` is empty.
pub fn save(
    app: &AppState,
    host: &str,
    username: &str,
    password: &str,
) -> Result<VaultView, ErrorView> {
    let host = host.trim().to_ascii_lowercase();
    let username = username.trim().to_owned();
    if host.is_empty() || username.is_empty() {
        return Err(ErrorView::new(
            "VAULT_ACCOUNT_INCOMPLETE",
            "The host and the username are required.",
            Some("Use plugins.svn.wordpress.org and your WordPress.org username.".to_owned()),
        ));
    }
    let mut accounts = vault::load_accounts(&app.paths).map_err(|e| coded(&e))?;
    let account = SvnAccount { host, username };
    let exists = accounts.contains(&account);
    if password.is_empty() && !exists {
        return Err(ErrorView::new(
            "VAULT_PASSWORD_REQUIRED",
            "Enter the SVN password for this account.",
            Some("Generate it on your WordPress.org profile under Account & Security.".to_owned()),
        ));
    }
    if !password.is_empty() {
        app.vault.set(&account.key(), &Secret::new(password)).map_err(|e| coded(&e))?;
    }
    if !exists {
        accounts.push(account);
        vault::save_accounts(&app.paths, &accounts).map_err(|e| coded(&e))?;
    }
    view(app)
}

/// Removes an account and its keychain entry.
pub fn remove(app: &AppState, host: &str, username: &str) -> Result<VaultView, ErrorView> {
    let mut accounts = vault::load_accounts(&app.paths).map_err(|e| coded(&e))?;
    let account = SvnAccount { host: host.to_owned(), username: username.to_owned() };
    app.vault.delete(&account.key()).map_err(|e| coded(&e))?;
    accounts.retain(|a| a != &account);
    vault::save_accounts(&app.paths, &accounts).map_err(|e| coded(&e))?;
    view(app)
}

/// Checks the keychain returns the password and the server answers with it.
///
/// WordPress.org lets anyone read, so it only checks a password on commit; the
/// message says so rather than claiming a verification that did not happen.
pub async fn test(app: &AppState, host: &str, username: &str) -> Result<String, ErrorView> {
    let account = SvnAccount { host: host.to_owned(), username: username.to_owned() };
    let Some(password) = app.vault.get(&account.key()).map_err(|e| coded(&e))? else {
        return Err(ErrorView::new(
            "VAULT_NO_CREDENTIALS",
            format!("No password is stored for {username}."),
            Some("Save the SVN password again.".to_owned()),
        ));
    };
    let configured = settings::load(&app.paths).map_err(|e| coded(&e))?.svn_path;
    let report = tools::discover_svn(configured.as_deref().map(std::path::Path::new)).await;
    let Some(bin) = report.path.clone().filter(|_| report.ok) else {
        return Err(ErrorView::new("TOOLS_SVN_UNAVAILABLE", report.message, report.fix));
    };
    let svn = Svn::new(PathBuf::from(bin), &NullReporter, CancellationToken::new());
    let credentials = Credentials { username: username.to_owned(), password };
    svn.list(&format!("https://{host}/"), Some(&credentials)).await.map_err(|e| coded(&e))?;
    Ok(format!(
        "The keychain returned the password and {host} answered. The server checks the password itself only when you publish."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_keeps_the_password_write_only_and_remove_deletes_it() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::test_support::app(dir.path());
        assert_eq!(
            save(&app, "plugins.svn.wordpress.org", "bob", "").unwrap_err().code,
            "VAULT_PASSWORD_REQUIRED"
        );
        let saved = save(&app, " Plugins.svn.wordpress.org ", "bob", "pw").unwrap();
        assert_eq!(saved.accounts.len(), 1);
        assert_eq!(saved.accounts[0].host, "plugins.svn.wordpress.org");
        assert!(saved.accounts[0].has_password);
        let json = serde_json::to_string(&saved).unwrap();
        assert!(!json.contains("\"pw\""));

        let unchanged = save(&app, "plugins.svn.wordpress.org", "bob", "").unwrap();
        assert!(unchanged.accounts[0].has_password);
        assert!(remove(&app, "plugins.svn.wordpress.org", "bob").unwrap().accounts.is_empty());
        assert!(app.vault.get("svn:plugins.svn.wordpress.org:bob").unwrap().is_none());
    }
}
