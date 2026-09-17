//! Provider records in `providers.json` (plan §9.6). No keys: those live in
//! the keychain under `ai:<id>`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::clock;
use crate::project::{self, AppPaths, ConfigError, SCHEMA};

use super::order;

/// A configured provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderRecord {
    /// `prov_<ulid>`.
    pub id: String,
    /// Adapter kind.
    pub kind: String,
    /// The developer's name for it.
    pub label: String,
    /// Model id (or Revoye provider constraint).
    pub model: String,
    /// Base URL override; `None` uses the adapter default.
    pub base_url: Option<String>,
    /// Whether a key is stored in the keychain.
    pub has_key: bool,
    /// Used when nothing else is chosen.
    pub is_default: bool,
    /// Set after an auth or payment failure, until cleared or a test passes.
    pub needs_attention: Option<String>,
    /// ISO 8601 UTC.
    pub created_at: String,
    /// Requests made in `usage_month`.
    pub requests_this_month: u32,
    /// `YYYY-MM` the count belongs to.
    #[serde(default)]
    pub usage_month: String,
}

/// `providers.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProvidersFile {
    /// Always 1.
    #[serde(default = "schema")]
    pub schema: u32,
    /// Records, in the order they were added.
    pub providers: Vec<ProviderRecord>,
    /// The opt-in fallback chain, in order; empty means off.
    #[serde(default)]
    pub fallback: Vec<String>,
}

fn schema() -> u32 {
    SCHEMA
}

/// A fresh provider id.
pub fn new_id() -> String {
    format!("prov_{}", ulid::Ulid::generate())
}

/// Loads the file, resetting counts from an earlier month, Revoye first.
pub fn load(paths: &AppPaths) -> Result<ProvidersFile, ConfigError> {
    let mut file =
        project::read_json::<ProvidersFile>(&paths.providers_file())?.unwrap_or_default();
    file.schema = SCHEMA;
    let month = clock::month(clock::now());
    for record in &mut file.providers {
        if record.usage_month != month {
            record.requests_this_month = 0;
            record.usage_month.clone_from(&month);
        }
    }
    file.providers = order::revoye_first(file.providers, |r| r.kind.as_str());
    file.fallback.retain(|id| file.providers.iter().any(|p| &p.id == id));
    Ok(file)
}

/// Saves the file.
pub fn save(paths: &AppPaths, file: &ProvidersFile) -> Result<(), ConfigError> {
    project::write_json(&paths.providers_file(), file)
}

/// Loads, changes and saves in one step.
pub fn update<T>(
    paths: &AppPaths,
    change: impl FnOnce(&mut ProvidersFile) -> T,
) -> Result<T, ConfigError> {
    let mut file = load(paths)?;
    let result = change(&mut file);
    save(paths, &file)?;
    Ok(result)
}

/// Counts one request against a record.
pub fn record_request(paths: &AppPaths, id: &str) -> Result<(), ConfigError> {
    update(paths, |file| {
        if let Some(record) = file.providers.iter_mut().find(|p| p.id == id) {
            record.requests_this_month = record.requests_this_month.saturating_add(1);
        }
    })
}

/// Marks a record as needing attention (auth or payment failures).
pub fn set_attention(
    paths: &AppPaths,
    id: &str,
    message: Option<String>,
) -> Result<(), ConfigError> {
    update(paths, |file| {
        if let Some(record) = file.providers.iter_mut().find(|p| p.id == id) {
            record.needs_attention = message;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, kind: &str) -> ProviderRecord {
        ProviderRecord {
            id: id.into(),
            kind: kind.into(),
            label: kind.into(),
            model: "m".into(),
            base_url: None,
            has_key: true,
            is_default: false,
            needs_attention: None,
            created_at: "2026-09-17T00:00:00Z".into(),
            requests_this_month: 7,
            usage_month: "2000-01".into(),
        }
    }

    #[test]
    fn load_orders_revoye_first_resets_old_counts_and_drops_stale_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        let file = ProvidersFile {
            schema: 1,
            providers: vec![record("a", "claude"), record("b", "revoye")],
            fallback: vec!["a".into(), "gone".into()],
        };
        save(&paths, &file).unwrap();
        let loaded = load(&paths).unwrap();
        assert_eq!(loaded.providers[0].kind, "revoye");
        assert_eq!(loaded.providers[0].requests_this_month, 0);
        assert_eq!(loaded.fallback, ["a"]);

        record_request(&paths, "a").unwrap();
        set_attention(&paths, "a", Some("Key rejected".into())).unwrap();
        let again = load(&paths).unwrap();
        let a = again.providers.iter().find(|p| p.id == "a").unwrap();
        assert_eq!(a.requests_this_month, 1);
        assert_eq!(a.needs_attention.as_deref(), Some("Key rejected"));
    }

    #[test]
    fn ids_are_prefixed_and_unique() {
        let (x, y) = (new_id(), new_id());
        assert!(x.starts_with("prov_") && x.len() == 31);
        assert_ne!(x, y);
    }
}
