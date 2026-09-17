//! Which provider runs, and the opt-in fallback chain (plan §9.5).

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::project::AppPaths;
use crate::secret::Secret;
use crate::vault::{self, CredentialStore};

use super::client::{AiClient, ClientError, Pending};
use super::provider::{
    ErrorCode, GenerateRequest, GenerateResult, Message, ProviderError, Role, WorkId,
};
use super::records::{self, ProviderRecord, ProvidersFile};
use super::registry;

/// Failures a different provider could survive. Safety, bad requests and
/// truncation never roll over: a different AI does not fix them.
pub const FALLBACK_CODES: [ErrorCode; 6] = [
    ErrorCode::Auth,
    ErrorCode::Payment,
    ErrorCode::RateLimit,
    ErrorCode::Server,
    ErrorCode::Network,
    ErrorCode::ModelNotFound,
];

/// The `NoProvider` message (plan §9.5 rule 5).
pub const NO_PROVIDER_MESSAGE: &str = "No AI provider configured. Add one on the Providers screen; Revoye is recommended, and a local model works without a key.";

fn no_provider(message: &str) -> ProviderError {
    ProviderError {
        code: ErrorCode::NoProvider,
        status: 0,
        retry_after: None,
        provider_kind: None,
        message: message.to_owned(),
    }
}

/// Resolves the provider: the per-run override, then the project's pinned
/// provider, then the default record, then the only record.
pub fn resolve(
    file: &ProvidersFile,
    override_id: Option<&str>,
    pinned_id: Option<&str>,
) -> Result<ProviderRecord, ProviderError> {
    if let Some(explicit) = override_id.or(pinned_id) {
        return file.providers.iter().find(|p| p.id == explicit).cloned().ok_or_else(|| {
            no_provider(
                "The chosen AI provider no longer exists. Choose another on the Providers screen.",
            )
        });
    }
    if let Some(default) = file.providers.iter().find(|p| p.is_default) {
        return Ok(default.clone());
    }
    match file.providers.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(no_provider(NO_PROVIDER_MESSAGE)),
    }
}

/// The user's chain after `failed_id`, skipping records that need attention.
pub fn fallback_chain(file: &ProvidersFile, failed_id: &str) -> Vec<ProviderRecord> {
    file.fallback
        .iter()
        .filter(|id| id.as_str() != failed_id)
        .filter_map(|id| file.providers.iter().find(|p| &p.id == id))
        .filter(|p| p.needs_attention.is_none())
        .cloned()
        .collect()
}

/// What to ask, independent of which provider answers.
#[derive(Debug, Clone, Copy)]
pub struct Ask<'a> {
    /// System prompt.
    pub system: &'a str,
    /// User prompt.
    pub user: &'a str,
    /// Required JSON shape.
    pub json_schema: Option<&'a Value>,
    /// Output ceiling.
    pub max_tokens: u32,
    /// The release work.
    pub work: Option<WorkId<'a>>,
}

/// Progress while routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteEvent {
    /// A provider is being asked.
    Attempt(ProviderRecord),
    /// Its job is still running.
    Pending(Pending),
}

/// The answer and who gave it.
#[derive(Debug, Clone, PartialEq)]
pub struct Routed {
    /// The generation.
    pub result: GenerateResult,
    /// The record that answered.
    pub provider: ProviderRecord,
    /// The first record, when a fallback answered instead.
    pub fell_back_from: Option<String>,
}

/// Everything a routed call needs.
pub struct Router<'a> {
    /// App data folders (records live there).
    pub paths: &'a AppPaths,
    /// The keychain.
    pub vault: &'a dyn CredentialStore,
    /// The HTTP client.
    pub client: &'a AiClient,
}

impl Router<'_> {
    async fn attempt(
        &self,
        record: &ProviderRecord,
        ask: &Ask<'_>,
        cancel: &CancellationToken,
        on_event: &(dyn Fn(RouteEvent) + Send + Sync),
    ) -> Result<GenerateResult, ClientError> {
        on_event(RouteEvent::Attempt(record.clone()));
        let adapter = registry::get(&record.kind)?;
        let key: Option<Secret> = if adapter.meta().keyless {
            None
        } else {
            let found = self.vault.get(&vault::ai_key(&record.id)).map_err(|e| {
                ProviderError::new(ErrorCode::Auth, adapter.meta().kind, e.to_string())
            })?;
            Some(found.ok_or_else(|| {
                ProviderError::new(
                    ErrorCode::Auth,
                    adapter.meta().kind,
                    format!("No API key is stored for {}.", record.label),
                )
            })?)
        };
        let messages = [Message { role: Role::User, content: ask.user.to_owned() }];
        let request = GenerateRequest {
            base_url: record.base_url.as_deref(),
            api_key: key.as_ref().map(Secret::expose),
            model: &record.model,
            system: Some(ask.system),
            messages: &messages,
            max_tokens: Some(ask.max_tokens),
            temperature: None,
            json_schema: ask.json_schema,
            work: ask.work,
        };
        let pending = |p: &Pending| on_event(RouteEvent::Pending(p.clone()));
        let result = self.client.generate(adapter, &request, cancel, &pending).await;
        drop(key);
        if !matches!(result, Err(ClientError::Cancelled))
            && records::record_request(self.paths, &record.id).is_err()
        {
            tracing::warn!("could not count the AI request");
        }
        if let Err(ClientError::Provider(e)) = &result
            && matches!(e.code, ErrorCode::Auth | ErrorCode::Payment)
            && records::set_attention(self.paths, &record.id, Some(e.message.clone())).is_err()
        {
            tracing::warn!("could not mark the provider as needing attention");
        }
        result
    }

    /// Asks `first`, then (only when the user built a chain) each fallback in order.
    pub async fn generate(
        &self,
        file: &ProvidersFile,
        first: &ProviderRecord,
        ask: &Ask<'_>,
        cancel: &CancellationToken,
        on_event: &(dyn Fn(RouteEvent) + Send + Sync),
    ) -> Result<Routed, ClientError> {
        let original = match self.attempt(first, ask, cancel, on_event).await {
            Ok(result) => {
                return Ok(Routed { result, provider: first.clone(), fell_back_from: None });
            }
            Err(ClientError::Provider(e)) if FALLBACK_CODES.contains(&e.code) => e,
            Err(other) => return Err(other),
        };
        for next in fallback_chain(file, &first.id) {
            match self.attempt(&next, ask, cancel, on_event).await {
                Ok(result) => {
                    return Ok(Routed {
                        result,
                        provider: next,
                        fell_back_from: Some(first.id.clone()),
                    });
                }
                Err(ClientError::Provider(e)) if FALLBACK_CODES.contains(&e.code) => {}
                Err(other) => return Err(other),
            }
        }
        Err(ClientError::Provider(original))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, default: bool) -> ProviderRecord {
        ProviderRecord {
            id: id.into(),
            kind: "local".into(),
            label: id.into(),
            model: "m".into(),
            base_url: None,
            has_key: false,
            is_default: default,
            needs_attention: None,
            created_at: String::new(),
            requests_this_month: 0,
            usage_month: String::new(),
        }
    }

    fn file(records: Vec<ProviderRecord>, fallback: &[&str]) -> ProvidersFile {
        ProvidersFile {
            schema: 1,
            providers: records,
            fallback: fallback.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn resolution_order() {
        let f = file(vec![record("a", false), record("b", true), record("c", false)], &[]);
        assert_eq!(resolve(&f, Some("c"), Some("a")).unwrap().id, "c");
        assert_eq!(resolve(&f, None, Some("a")).unwrap().id, "a");
        assert_eq!(resolve(&f, None, None).unwrap().id, "b");
        assert_eq!(resolve(&f, Some("gone"), None).unwrap_err().code, ErrorCode::NoProvider);

        let only = file(vec![record("x", false)], &[]);
        assert_eq!(resolve(&only, None, None).unwrap().id, "x");
        let many = file(vec![record("x", false), record("y", false)], &[]);
        assert_eq!(resolve(&many, None, None).unwrap_err().message, NO_PROVIDER_MESSAGE);
        assert_eq!(
            resolve(&file(vec![], &[]), None, None).unwrap_err().code,
            ErrorCode::NoProvider
        );
    }

    #[test]
    fn fallback_chain_is_opt_in_ordered_and_skips_attention() {
        let mut broken = record("c", false);
        broken.needs_attention = Some("Key rejected".into());
        let f = file(vec![record("a", true), record("b", false), broken], &["c", "a", "b"]);
        let chain: Vec<String> = fallback_chain(&f, "a").into_iter().map(|p| p.id).collect();
        assert_eq!(chain, ["b"]);
        assert!(
            fallback_chain(&file(vec![record("a", true), record("b", false)], &[]), "a").is_empty()
        );
    }

    #[test]
    fn only_recoverable_codes_roll_over() {
        for code in
            [ErrorCode::Safety, ErrorCode::BadRequest, ErrorCode::Truncated, ErrorCode::NoProvider]
        {
            assert!(!FALLBACK_CODES.contains(&code));
        }
    }
}
