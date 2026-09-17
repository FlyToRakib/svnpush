//! The provider contract (plan §9.1): every adapter is a pure request builder
//! and response parser. Only `ai::client` sends requests.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::error::Coded;

/// What the Providers screen and the router know about an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterMeta {
    /// Registry key: `revoye`, `gemini`, `claude`, `openai`, …
    pub kind: &'static str,
    /// Human name.
    pub label: &'static str,
    /// Shows "(Recommended)" in the picker; true only for Revoye.
    pub recommended: bool,
    /// Where requests go unless the record sets another base URL.
    pub default_base_url: &'static str,
    /// Hides the Base URL field (first-party endpoints).
    pub fixed_base_url: bool,
    /// The model pre-filled for a new record.
    pub default_model: &'static str,
    /// A closed list makes the model field a dropdown; empty means free text.
    pub models: &'static [&'static str],
    /// Local models need no key.
    pub keyless: bool,
    /// Placeholder for the API key field.
    pub key_placeholder: &'static str,
    /// One or two sentences under the provider picker.
    pub note: &'static str,
    /// One line under the model field.
    pub model_hint: &'static str,
}

/// Who wrote a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The developer's side.
    User,
    /// The model's side.
    Assistant,
}

/// One conversation turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// The author.
    pub role: Role,
    /// Plain text.
    pub content: String,
}

/// The work a request belongs to, for idempotency keys and metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkId<'a> {
    /// Plugin slug.
    pub slug: &'a str,
    /// Version being released.
    pub version: &'a str,
    /// Task name: `draft_release`, `summarise_file`, `explain_failures`.
    pub task: &'a str,
}

/// A normalised generation request.
#[derive(Debug, Clone, Copy)]
pub struct GenerateRequest<'a> {
    /// The record's base URL, when it overrides the default.
    pub base_url: Option<&'a str>,
    /// The API key; `None` for keyless adapters.
    pub api_key: Option<&'a str>,
    /// Model id (or Revoye provider constraint).
    pub model: &'a str,
    /// System instructions.
    pub system: Option<&'a str>,
    /// The conversation.
    pub messages: &'a [Message],
    /// Output token ceiling.
    pub max_tokens: Option<u32>,
    /// Sampling temperature, sent only where the dialect accepts it.
    pub temperature: Option<f32>,
    /// Requested JSON schema for the answer.
    pub json_schema: Option<&'a Value>,
    /// The release work this belongs to; `None` for connection tests.
    pub work: Option<WorkId<'a>>,
}

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// GET
    Get,
    /// POST
    Post,
    /// DELETE
    Delete,
}

/// A request for the client to send.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequest {
    /// Method.
    pub method: Method,
    /// Full URL.
    pub url: String,
    /// Headers. Values may hold the key: never log them.
    pub headers: Vec<(String, String)>,
    /// JSON body.
    pub body: Option<Value>,
}

impl HttpRequest {
    /// A request with no body.
    pub fn get(url: String, headers: Vec<(String, String)>) -> Self {
        Self { method: Method::Get, url, headers, body: None }
    }

    /// A JSON POST.
    pub fn post(url: String, headers: Vec<(String, String)>, body: Value) -> Self {
        Self { method: Method::Post, url, headers, body: Some(body) }
    }
}

/// Why generation stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum FinishReason {
    /// Finished normally.
    Stop,
    /// Hit the output limit.
    Length,
    /// Stopped by a safety system.
    Safety,
    /// Anything else, as reported.
    Other(String),
}

/// A completed generation.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateResult {
    /// The answer text.
    pub text: String,
    /// Prompt tokens; `None` (never 0) when not reported.
    pub tokens_in: Option<u32>,
    /// Output tokens; `None` when not reported.
    pub tokens_out: Option<u32>,
    /// Why it stopped.
    pub finish_reason: FinishReason,
    /// The raw response, for tests and diagnostics (never logged).
    pub raw: Value,
}

/// A response that finished, or a job still running.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseOutcome {
    /// The answer.
    Complete(GenerateResult),
    /// Accepted and still running; poll it.
    Pending {
        /// Job id.
        job_id: String,
        /// Provider status text.
        status: String,
        /// Queue depth ahead, when reported.
        queue_position: Option<u32>,
    },
}

/// Normalised failure codes, identical to SyncDock's (plan §9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ErrorCode {
    /// Invalid, expired or revoked key.
    Auth,
    /// Too many requests; `retry_after` when known.
    RateLimit,
    /// Provider-side failure or no capacity.
    Server,
    /// Out of credits.
    Payment,
    /// Output cut at the length limit.
    Truncated,
    /// The model refused.
    Safety,
    /// Unknown or retired model.
    ModelNotFound,
    /// The request could not be sent or the local server is not running.
    Network,
    /// Malformed request or oversized prompt.
    BadRequest,
    /// Nothing is configured.
    NoProvider,
}

impl ErrorCode {
    /// The stable string code.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "AI_AUTH",
            Self::RateLimit => "AI_RATE_LIMIT",
            Self::Server => "AI_SERVER",
            Self::Payment => "AI_PAYMENT",
            Self::Truncated => "AI_TRUNCATED",
            Self::Safety => "AI_SAFETY",
            Self::ModelNotFound => "AI_MODEL_NOT_FOUND",
            Self::Network => "AI_NETWORK",
            Self::BadRequest => "AI_BAD_REQUEST",
            Self::NoProvider => "AI_NO_PROVIDER",
        }
    }
}

/// A normalised provider failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ProviderError {
    /// Category.
    pub code: ErrorCode,
    /// HTTP status (0 when none).
    pub status: u16,
    /// Seconds to wait, when the provider said.
    pub retry_after: Option<u32>,
    /// The adapter kind.
    pub provider_kind: Option<&'static str>,
    /// Human message, never containing the key.
    pub message: String,
}

impl ProviderError {
    /// A failure with a code and message.
    pub fn new(code: ErrorCode, kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            status: 0,
            retry_after: None,
            provider_kind: Some(kind),
            message: message.into(),
        }
    }

    /// Sets the HTTP status.
    #[must_use]
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }
}

impl Coded for ProviderError {
    fn code(&self) -> &'static str {
        self.code.as_str()
    }

    fn fix(&self) -> Option<String> {
        Some(
            match self.code {
                ErrorCode::Auth => "Create a new API key with the provider and paste it on the Providers screen.",
                ErrorCode::RateLimit => "Wait a moment and retry, or choose another provider.",
                ErrorCode::Server => "Retry, or choose another provider.",
                ErrorCode::Payment => "Add credits with the provider, or choose another provider.",
                ErrorCode::Truncated => "Retry; if it repeats, the diff is too large for this model.",
                ErrorCode::Safety => "Write the release notes by hand, or rephrase and retry.",
                ErrorCode::ModelNotFound => "Choose a current model for this provider on the Providers screen.",
                ErrorCode::Network => "Check your connection, or start the local model server.",
                ErrorCode::BadRequest => "Check the provider settings; if they are right, write the notes by hand.",
                ErrorCode::NoProvider => "Add a provider on the Providers screen. Revoye is recommended, and a local model works without a key.",
            }
            .to_owned(),
        )
    }
}

/// The default failure for a bare HTTP status.
pub fn error_from_status(status: u16, message: String, kind: &'static str) -> ProviderError {
    let code = match status {
        401 | 403 => ErrorCode::Auth,
        402 => ErrorCode::Payment,
        404 => ErrorCode::ModelNotFound,
        429 => ErrorCode::RateLimit,
        s if s >= 500 => ErrorCode::Server,
        _ => ErrorCode::BadRequest,
    };
    let message =
        if message.is_empty() { format!("Provider request failed ({status})") } else { message };
    ProviderError { code, status, retry_after: None, provider_kind: Some(kind), message }
}

/// Removes trailing slashes so paths join cleanly.
pub fn trim_base(url: &str) -> String {
    url.trim_end_matches('/').to_owned()
}

/// Models an account can use.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ModelList {
    /// Model ids.
    pub models: Vec<String>,
    /// Why the list is empty, when the provider says.
    pub note: Option<String>,
}

/// One Revoye provider's capacity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FleetProvider {
    /// Provider kind.
    pub kind: String,
    /// Switched on.
    pub enabled: bool,
    /// Agents the router could dispatch to now.
    pub agents_idle: u32,
}

/// A fleet snapshot (plan §9.3).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Fleet {
    /// Active devices.
    pub devices_total: u32,
    /// Devices connected now.
    pub devices_online: u32,
    /// All agents.
    pub agents_total: u32,
    /// Idle agents.
    pub agents_idle: u32,
    /// Jobs waiting.
    pub queue_depth: u32,
    /// Per-provider capacity.
    pub providers: Vec<FleetProvider>,
}

/// Listing the models an account can use.
pub trait ListModels: Send + Sync {
    /// The request.
    fn build_request(&self, base_url: Option<&str>, api_key: Option<&str>) -> HttpRequest;
    /// The parsed list.
    fn parse_response(&self, json: &Value) -> ModelList;
}

/// Polling a pending job.
pub trait Poll: Send + Sync {
    /// Milliseconds between polls.
    fn interval_ms(&self) -> u64;
    /// Ceiling for the whole job.
    fn max_ms(&self) -> u64;
    /// The poll request.
    fn build_request(&self, api_key: &str, job_id: &str) -> HttpRequest;
    /// The job's state.
    fn parse_response(&self, json: &Value) -> Result<ParseOutcome, ProviderError>;
    /// Cancels the job on the server.
    fn build_cancel(&self, api_key: &str, job_id: &str) -> HttpRequest;
}

/// A capacity snapshot.
pub trait FleetStatus: Send + Sync {
    /// The request.
    fn build_request(&self, api_key: &str) -> HttpRequest;
    /// The parsed snapshot.
    fn parse_response(&self, json: &Value) -> Fleet;
}

/// One AI provider's dialect.
pub trait Adapter: Send + Sync {
    /// Static description.
    fn meta(&self) -> &AdapterMeta;
    /// The generation request.
    fn build_request(&self, req: &GenerateRequest<'_>) -> Result<HttpRequest, ProviderError>;
    /// A 2xx response.
    fn parse_response(&self, json: &Value) -> Result<ParseOutcome, ProviderError>;
    /// A non-2xx response.
    fn parse_error(&self, status: u16, json: &Value) -> ProviderError;
    /// Live model listing, when supported.
    fn list_models(&self) -> Option<&dyn ListModels> {
        None
    }
    /// Job polling, when the provider is asynchronous.
    fn poll(&self) -> Option<&dyn Poll> {
        None
    }
    /// Capacity snapshot, when supported.
    fn fleet_status(&self) -> Option<&dyn FleetStatus> {
        None
    }
}

/// `json[key]` as a string, or empty.
pub(crate) fn str_at<'a>(json: &'a Value, pointer: &str) -> Option<&'a str> {
    json.pointer(pointer).and_then(Value::as_str)
}

/// `json[pointer]` as a u32.
pub(crate) fn u32_at(json: &Value, pointer: &str) -> Option<u32> {
    json.pointer(pointer).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping() {
        let code = |s| error_from_status(s, String::new(), "x").code;
        assert_eq!(code(401), ErrorCode::Auth);
        assert_eq!(code(403), ErrorCode::Auth);
        assert_eq!(code(402), ErrorCode::Payment);
        assert_eq!(code(404), ErrorCode::ModelNotFound);
        assert_eq!(code(429), ErrorCode::RateLimit);
        assert_eq!(code(503), ErrorCode::Server);
        assert_eq!(code(400), ErrorCode::BadRequest);
        assert_eq!(
            error_from_status(500, String::new(), "x").message,
            "Provider request failed (500)"
        );
    }

    #[test]
    fn trims_trailing_slashes() {
        assert_eq!(trim_base("https://a/v1//"), "https://a/v1");
    }
}
