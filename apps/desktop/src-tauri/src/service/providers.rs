//! The Providers screen: records, keys, Test, model lists, fleet, fallback.

use serde::{Deserialize, Serialize};
use svnpush_core::ai::client::ClientError;
use svnpush_core::ai::provider::{Adapter, Fleet, GenerateRequest, Message, ModelList, Role};
use svnpush_core::ai::records::{self, ProviderRecord, ProvidersFile};
use svnpush_core::ai::registry;
use svnpush_core::clock;
use svnpush_core::run::ErrorView;
use svnpush_core::secret::Secret;
use svnpush_core::vault;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::state::AppState;

/// An adapter's description, as the form needs it. The flags mirror
/// independent form rules in `AdapterMeta`, so they stay separate booleans.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AdapterInfo {
    /// Registry key.
    pub kind: String,
    /// Human name.
    pub label: String,
    /// Shows "(Recommended)".
    pub recommended: bool,
    /// Default base URL.
    pub default_base_url: String,
    /// Hides the Base URL field.
    pub fixed_base_url: bool,
    /// Default model.
    pub default_model: String,
    /// A closed list shows a dropdown.
    pub models: Vec<String>,
    /// No API key needed.
    pub keyless: bool,
    /// Key placeholder.
    pub key_placeholder: String,
    /// Note under the picker.
    pub note: String,
    /// Hint under the model field.
    pub model_hint: String,
    /// Whether ↻ can load the account's models.
    pub can_list_models: bool,
    /// Whether a fleet line is available.
    pub has_fleet: bool,
}

/// What the Add/Edit form submits. `api_key` is write-only.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export)]
pub struct ProviderInput {
    /// `None` adds a record.
    pub id: Option<String>,
    /// Adapter kind.
    pub kind: String,
    /// Label; empty uses the adapter's name.
    pub label: String,
    /// Model; empty uses the adapter default.
    pub model: String,
    /// Base URL override.
    pub base_url: Option<String>,
    /// Make this the default.
    pub is_default: bool,
    /// A new key; `None` or empty keeps the stored one.
    pub api_key: Option<String>,
}

/// Which account ↻ and the fleet line ask about: a saved record or a key being typed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export)]
pub struct ProviderTarget {
    /// A saved record.
    pub id: Option<String>,
    /// Adapter kind.
    pub kind: String,
    /// Base URL override.
    pub base_url: Option<String>,
    /// A key typed but not saved.
    pub api_key: Option<String>,
}

fn coded(e: &impl svnpush_core::Coded) -> ErrorView {
    ErrorView::from_coded(e)
}

fn info(adapter: &dyn Adapter) -> AdapterInfo {
    let m = adapter.meta();
    AdapterInfo {
        kind: m.kind.to_owned(),
        label: m.label.to_owned(),
        recommended: m.recommended,
        default_base_url: m.default_base_url.to_owned(),
        fixed_base_url: m.fixed_base_url,
        default_model: m.default_model.to_owned(),
        models: m.models.iter().map(|s| (*s).to_owned()).collect(),
        keyless: m.keyless,
        key_placeholder: m.key_placeholder.to_owned(),
        note: m.note.to_owned(),
        model_hint: m.model_hint.to_owned(),
        can_list_models: adapter.list_models().is_some(),
        has_fleet: adapter.fleet_status().is_some(),
    }
}

/// Every adapter, Revoye first.
pub fn adapters() -> Vec<AdapterInfo> {
    registry::list().into_iter().map(info).collect()
}

/// The records and fallback chain.
pub fn list(app: &AppState) -> Result<ProvidersFile, ErrorView> {
    records::load(&app.paths).map_err(|e| coded(&e))
}

/// Adds or edits a record; a new key goes straight to the keychain.
pub fn save(app: &AppState, input: ProviderInput) -> Result<ProvidersFile, ErrorView> {
    let adapter = registry::get(&input.kind).map_err(|e| coded(&e))?;
    let meta = adapter.meta();
    let key = input.api_key.as_deref().map(str::trim).filter(|k| !k.is_empty());
    let mut file = records::load(&app.paths).map_err(|e| coded(&e))?;
    let existing = input.id.as_ref().and_then(|id| file.providers.iter().position(|p| &p.id == id));
    let has_stored_key = existing.is_some_and(|i| file.providers[i].has_key);
    if !meta.keyless && key.is_none() && !has_stored_key {
        return Err(ErrorView::new(
            "PROVIDER_KEY_REQUIRED",
            "An API key is required for this provider.",
            Some("Paste the key from the provider's dashboard.".to_owned()),
        ));
    }
    let id = input.id.clone().unwrap_or_else(records::new_id);
    if meta.keyless {
        app.vault.delete(&vault::ai_key(&id)).map_err(|e| coded(&e))?;
    } else if let Some(key) = key {
        app.vault.set(&vault::ai_key(&id), &Secret::new(key)).map_err(|e| coded(&e))?;
    }
    let label = if input.label.trim().is_empty() {
        meta.label.to_owned()
    } else {
        input.label.trim().to_owned()
    };
    let model = if input.model.trim().is_empty() {
        meta.default_model.to_owned()
    } else {
        input.model.trim().to_owned()
    };
    let base_url =
        if meta.fixed_base_url { None } else { input.base_url.filter(|b| !b.trim().is_empty()) };
    if let Some(i) = existing {
        let current = &mut file.providers[i];
        current.kind.clone_from(&input.kind);
        current.label = label;
        current.model = model;
        current.base_url = base_url;
        current.has_key = !meta.keyless && (key.is_some() || has_stored_key);
    } else {
        file.providers.push(ProviderRecord {
            id: id.clone(),
            kind: input.kind.clone(),
            label,
            model,
            base_url,
            has_key: !meta.keyless,
            is_default: false,
            needs_attention: None,
            created_at: clock::iso8601(clock::now()),
            requests_this_month: 0,
            usage_month: clock::month(clock::now()),
        });
    }
    let record = id;
    let only_one = file.providers.len() == 1;
    if input.is_default || only_one {
        for p in &mut file.providers {
            p.is_default = p.id == record;
        }
    } else if let Some(p) = file.providers.iter_mut().find(|p| p.id == record) {
        p.is_default = false;
    }
    records::save(&app.paths, &file).map_err(|e| coded(&e))?;
    list(app)
}

/// Removes a record, its key and its place in the fallback chain.
pub fn remove(app: &AppState, id: &str) -> Result<ProvidersFile, ErrorView> {
    app.vault.delete(&vault::ai_key(id)).map_err(|e| coded(&e))?;
    records::update(&app.paths, |file| {
        file.providers.retain(|p| p.id != id);
        file.fallback.retain(|f| f != id);
    })
    .map_err(|e| coded(&e))?;
    list(app)
}

/// Makes one record the default.
pub fn set_default(app: &AppState, id: &str) -> Result<ProvidersFile, ErrorView> {
    records::update(&app.paths, |file| {
        for p in &mut file.providers {
            p.is_default = p.id == id;
        }
    })
    .map_err(|e| coded(&e))?;
    list(app)
}

/// Clears "needs attention".
pub fn clear_attention(app: &AppState, id: &str) -> Result<ProvidersFile, ErrorView> {
    records::set_attention(&app.paths, id, None).map_err(|e| coded(&e))?;
    list(app)
}

/// Saves the opt-in fallback order; empty turns fallback off.
pub fn save_fallback(app: &AppState, order: Vec<String>) -> Result<ProvidersFile, ErrorView> {
    records::update(&app.paths, |file| {
        file.fallback =
            order.into_iter().filter(|id| file.providers.iter().any(|p| &p.id == id)).collect();
    })
    .map_err(|e| coded(&e))?;
    list(app)
}

fn key_for(app: &AppState, target: &ProviderTarget) -> Result<Option<Secret>, ErrorView> {
    if let Some(key) = target.api_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        return Ok(Some(Secret::new(key)));
    }
    match &target.id {
        Some(id) => app.vault.get(&vault::ai_key(id)).map_err(|e| coded(&e)),
        None => Ok(None),
    }
}

/// The models the account behind a key can use.
pub async fn list_models(app: &AppState, target: ProviderTarget) -> Result<ModelList, ErrorView> {
    let adapter = registry::get(&target.kind).map_err(|e| coded(&e))?;
    let key = key_for(app, &target)?;
    app.ai
        .list_models(adapter, target.base_url.as_deref(), key.as_ref().map(Secret::expose))
        .await
        .map_err(|e| coded(&e))
}

/// The fleet snapshot for a Revoye key.
pub async fn fleet(app: &AppState, target: ProviderTarget) -> Result<Fleet, ErrorView> {
    let adapter = registry::get(&target.kind).map_err(|e| coded(&e))?;
    let key = key_for(app, &target)?.ok_or_else(|| {
        ErrorView::new(
            "PROVIDER_KEY_REQUIRED",
            "Paste the API key first.",
            Some("The key names the account.".to_owned()),
        )
    })?;
    app.ai.fleet_status(adapter, key.expose()).await.map_err(|e| coded(&e))
}

/// Checks a saved record works, and clears "needs attention" when it does.
///
/// Revoye is checked with its status endpoint, which proves the key without
/// taking an agent's time; every other provider answers a one-word prompt.
pub async fn test(app: &AppState, id: &str) -> Result<String, ErrorView> {
    let file = list(app)?;
    let record = file.providers.iter().find(|p| p.id == id).cloned().ok_or_else(|| {
        ErrorView::new("PROVIDER_NOT_FOUND", "That provider no longer exists.", None)
    })?;
    let adapter = registry::get(&record.kind).map_err(|e| coded(&e))?;
    let key = app.vault.get(&vault::ai_key(id)).map_err(|e| coded(&e))?;
    if !adapter.meta().keyless && key.is_none() {
        return Err(ErrorView::new(
            "PROVIDER_KEY_REQUIRED",
            "No API key is stored for this provider.",
            Some("Edit the provider and paste its key.".to_owned()),
        ));
    }
    let message = if adapter.fleet_status().is_some() {
        let fleet = app
            .ai
            .fleet_status(adapter, key.as_ref().map_or("", Secret::expose))
            .await
            .map_err(|e| coded(&e))?;
        format!(
            "Connected. {} of {} device(s) online, {} of {} agents free.",
            fleet.devices_online, fleet.devices_total, fleet.agents_idle, fleet.agents_total
        )
    } else {
        let messages =
            [Message { role: Role::User, content: "Reply with the single word OK.".to_owned() }];
        let request = GenerateRequest {
            base_url: record.base_url.as_deref(),
            api_key: key.as_ref().map(Secret::expose),
            model: &record.model,
            system: None,
            messages: &messages,
            max_tokens: Some(256),
            temperature: None,
            json_schema: None,
            work: None,
        };
        let result = app.ai.generate(adapter, &request, &CancellationToken::new(), &|_| {}).await;
        records::record_request(&app.paths, id).map_err(|e| coded(&e))?;
        match result {
            Ok(_) => format!("Connected. {} answered with {}.", record.label, record.model),
            Err(ClientError::Cancelled) => {
                return Err(ErrorView::new("CANCELLED", "The test was cancelled.", None));
            }
            Err(ClientError::Provider(e)) => return Err(coded(&e)),
        }
    };
    records::set_attention(&app.paths, id, None).map_err(|e| coded(&e))?;
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: &str, key: Option<&str>) -> ProviderInput {
        ProviderInput {
            id: None,
            kind: kind.into(),
            label: String::new(),
            model: String::new(),
            base_url: Some("https://ignored.example".into()),
            is_default: false,
            api_key: key.map(str::to_owned),
        }
    }

    #[test]
    fn adapters_follow_their_meta() {
        let all = adapters();
        assert_eq!(all[0].kind, "revoye");
        assert!(
            all[0].recommended
                && all[0].fixed_base_url
                && all[0].can_list_models
                && all[0].has_fleet
        );
        let local = all.iter().find(|a| a.kind == "local").unwrap();
        assert!(local.keyless && !local.can_list_models);
    }

    #[test]
    fn records_keys_defaults_and_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::test_support::app(dir.path());
        assert_eq!(save(&app, input("claude", None)).unwrap_err().code, "PROVIDER_KEY_REQUIRED");

        let first = save(&app, input("revoye", Some("revoye_sk_live_abc"))).unwrap();
        let revoye = first.providers[0].clone();
        assert!(revoye.is_default, "the only record is the default");
        assert_eq!(revoye.model, "revoye/auto");
        assert_eq!(revoye.base_url, None, "fixed base URLs are never stored");
        assert_eq!(
            app.vault.get(&vault::ai_key(&revoye.id)).unwrap().unwrap().expose(),
            "revoye_sk_live_abc"
        );
        assert!(!serde_json::to_string(&first).unwrap().contains("revoye_sk_live_abc"));

        let second = save(&app, input("local", None)).unwrap();
        let local = second.providers.iter().find(|p| p.kind == "local").unwrap().clone();
        assert!(!local.has_key && !local.is_default);
        assert_eq!(local.base_url.as_deref(), Some("https://ignored.example"));

        let mut edit = input("revoye", None);
        edit.id = Some(revoye.id.clone());
        edit.label = "Mine".into();
        let edited = save(&app, edit).unwrap();
        assert_eq!(edited.providers[0].label, "Mine");
        assert!(edited.providers[0].has_key, "an empty key keeps the stored one");

        set_default(&app, &local.id).unwrap();
        save_fallback(&app, vec![local.id.clone(), "gone".into()]).unwrap();
        let after = remove(&app, &revoye.id).unwrap();
        assert_eq!(after.providers.len(), 1);
        assert!(after.providers[0].is_default);
        assert_eq!(after.fallback, [local.id]);
        assert!(app.vault.get(&vault::ai_key(&revoye.id)).unwrap().is_none());
    }
}
