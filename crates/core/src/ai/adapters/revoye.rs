//! Revoye (plan §9.3), written from `docs/REVOYE-API.md` only.
//!
//! Revoye runs the prompt through an AI account the developer is already
//! signed into, in a real browser on their machine. A job takes 20 to 90
//! seconds, so it is submitted with `wait: false` and polled. `model` is a
//! provider constraint (`revoye/auto` or a kind such as `chatgpt`), never a
//! model id. There are no token counts.

use serde_json::{Value, json};

use crate::ai::provider::{
    Adapter, AdapterMeta, ErrorCode, FinishReason, Fleet, FleetProvider, FleetStatus,
    GenerateRequest, GenerateResult, HttpRequest, ListModels, Method, ModelList, ParseOutcome,
    Poll, ProviderError, Role, error_from_status, str_at, u32_at,
};

const KIND: &str = "revoye";
/// Fixed on purpose: never taken from the user.
const BASE_URL: &str = "https://api.revoye.com";
const AUTO: &str = "revoye/auto";
const PROVIDER_KINDS: [&str; 6] = ["chatgpt", "claude", "gemini", "deepseek", "qwen", "perplexity"];
const MODELS: [&str; 7] = [AUTO, "chatgpt", "claude", "gemini", "deepseek", "qwen", "perplexity"];
/// `prompt` is 1–100 000 characters (§5).
pub const MAX_PROMPT_CHARS: usize = 100_000;
/// Per attempt.
const ATTEMPT_TIMEOUT_MS: u64 = 180_000;
/// Whole job, matching the poll ceiling.
const JOB_DEADLINE_MS: u64 = 600_000;

static META: AdapterMeta = AdapterMeta {
    kind: KIND,
    label: "Revoye",
    recommended: true,
    default_base_url: BASE_URL,
    fixed_base_url: true,
    default_model: AUTO,
    models: &MODELS,
    keyless: false,
    key_placeholder: "revoye_sk_live_…",
    note: "Answers come from AI accounts you are already signed into, through Revoye Desk on your machine. A draft takes 20 to 90 seconds.",
    model_hint: "revoye/auto lets Revoye choose; pick a kind to pin it. Use ↻ to load the providers your account has enabled.",
};

/// The Revoye adapter.
pub struct Revoye;

/// The Revoye adapter instance.
pub static REVOYE: Revoye = Revoye;

fn err(code: ErrorCode, message: impl Into<String>) -> ProviderError {
    ProviderError::new(code, KIND, message)
}

fn auth(api_key: &str) -> Vec<(String, String)> {
    vec![("authorization".to_owned(), format!("Bearer {api_key}"))]
}

/// `model` → the `provider` constraint; `None` means any enabled provider.
fn provider_constraint(model: &str) -> Result<Option<&'static str>, ProviderError> {
    let trimmed = model.trim().to_ascii_lowercase();
    let bare = trimmed.strip_prefix("revoye/").unwrap_or(&trimmed);
    if bare.is_empty() || bare == "auto" {
        return Ok(None);
    }
    PROVIDER_KINDS.iter().find(|k| **k == bare).map(|k| Some(*k)).ok_or_else(|| {
        err(
            ErrorCode::BadRequest,
            format!(
                "\"{model}\" is not a Revoye provider. Use {AUTO} or one of: {}.",
                PROVIDER_KINDS.join(", ")
            ),
        )
    })
}

/// System text first, then each turn labelled (a lone user turn is not),
/// joined by blank lines; a JSON schema becomes a closing instruction.
pub fn flatten_prompt(req: &GenerateRequest<'_>) -> Result<String, ProviderError> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(system) = req.system.filter(|s| !s.is_empty()) {
        parts.push(system.to_owned());
    }
    match req.messages {
        [only] if only.role == Role::User => parts.push(only.content.clone()),
        turns => {
            for m in turns {
                let label = if m.role == Role::Assistant { "Assistant" } else { "User" };
                parts.push(format!("{label}: {}", m.content));
            }
        }
    }
    if let Some(schema) = req.json_schema {
        parts.push(format!(
            "Reply with a single valid JSON object matching this schema and nothing else: no prose, no explanation, no Markdown code fences.\n{schema}"
        ));
    }
    let prompt = parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n");
    if prompt.trim().is_empty() {
        return Err(err(ErrorCode::BadRequest, "Nothing to send: the prompt is empty."));
    }
    let chars = prompt.chars().count();
    if chars > MAX_PROMPT_CHARS {
        return Err(err(
            ErrorCode::BadRequest,
            format!("The prompt is {chars} characters; Revoye accepts at most {MAX_PROMPT_CHARS}."),
        ));
    }
    Ok(prompt)
}

/// Derived from the work, so a crash and retry gets the same job back (plan §9.3).
pub fn idempotency_key(req: &GenerateRequest<'_>, prompt: &str) -> String {
    let hash = blake3::hash(prompt.as_bytes()).to_hex();
    if let Some(work) = req.work {
        return format!("svnpush:{}:{}:{}:{}", work.slug, work.version, work.task, &hash[..16]);
    }
    // A connection test is new work every time.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("svnpush:test:{nonce}:{}", &hash[..16])
}

/// One job object (submit and poll responses share it) → an outcome.
fn parse_job(job: &Value) -> Result<ParseOutcome, ProviderError> {
    let status = str_at(job, "/status").unwrap_or_default();
    match status {
        "succeeded" => {
            let response = job.get("response").and_then(Value::as_str);
            match (response, str_at(job, "/content_pruned_at")) {
                (None, Some(pruned)) => Err(err(
                    ErrorCode::Server,
                    format!("Revoye removed this answer's text at {pruned} under your retention setting. Run it again."),
                )
                .with_status(200)),
                (text, _) => Ok(ParseOutcome::Complete(GenerateResult {
                    text: text.unwrap_or_default().to_owned(),
                    tokens_in: None,
                    tokens_out: None,
                    finish_reason: FinishReason::Stop,
                    raw: job.clone(),
                })),
            }
        }
        "failed" => Err(err(
            ErrorCode::Server,
            format!(
                "The Revoye job failed after {} attempt(s).",
                u32_at(job, "/attempts").unwrap_or(1)
            ),
        )
        .with_status(502)),
        "expired" => {
            Err(err(ErrorCode::Server, "The Revoye job expired before an agent finished it.")
                .with_status(504))
        }
        "cancelled" => {
            Err(err(ErrorCode::Server, "The Revoye job was cancelled.").with_status(499))
        }
        _ => Ok(ParseOutcome::Pending {
            job_id: str_at(job, "/id").unwrap_or_default().to_owned(),
            status: status.to_owned(),
            queue_position: u32_at(job, "/queue_position"),
        }),
    }
}

impl Adapter for Revoye {
    fn meta(&self) -> &AdapterMeta {
        &META
    }

    fn build_request(&self, req: &GenerateRequest<'_>) -> Result<HttpRequest, ProviderError> {
        let api_key =
            req.api_key.ok_or_else(|| err(ErrorCode::Auth, "Revoye needs an API key."))?;
        let provider = provider_constraint(req.model)?;
        let prompt = flatten_prompt(req)?;
        let mut headers = auth(api_key);
        headers.push(("content-type".to_owned(), "application/json".to_owned()));
        headers.push(("idempotency-key".to_owned(), idempotency_key(req, &prompt)));
        let task = req.work.map_or("test", |w| w.task);
        Ok(HttpRequest::post(
            format!("{BASE_URL}/v1/completions"),
            headers,
            json!({
                "prompt": prompt,
                "provider": provider,
                "wait": false,
                "timeout_ms": ATTEMPT_TIMEOUT_MS,
                "deadline_ms": JOB_DEADLINE_MS,
                "metadata": { "source": "svnpush", "task": task },
            }),
        ))
    }

    fn parse_response(&self, json: &Value) -> Result<ParseOutcome, ProviderError> {
        parse_job(json)
    }

    fn parse_error(&self, status: u16, json: &Value) -> ProviderError {
        let code = str_at(json, "/error/code").unwrap_or_default();
        let details = json.pointer("/error/details");
        let detail_u32 = |key: &str| details.and_then(|d| d.get(key)).and_then(Value::as_u64);
        let made = |code: ErrorCode, message: String| err(code, message).with_status(status);
        match code {
            "UNAUTHORIZED" => made(ErrorCode::Auth, "Revoye rejected the API key. Create a new one at revoye.com.".to_owned()),
            "FORBIDDEN" => {
                // Same status, two causes: the queue ceiling is "come back later",
                // a missing scope is a key problem.
                if let Some(limit) = detail_u32("limit") {
                    made(ErrorCode::RateLimit, format!("Your Revoye queue is full ({limit} jobs). Wait for it to drain."))
                } else {
                    let scope =
                        details.and_then(|d| d.get("required_scope")).and_then(Value::as_str).unwrap_or("required");
                    made(ErrorCode::Auth, format!("The Revoye key is missing the {scope} scope."))
                }
            }
            "RATE_LIMITED" => {
                let mut e = made(ErrorCode::RateLimit, "Sending to Revoye too fast.".to_owned());
                e.retry_after = detail_u32("retry_after_seconds").and_then(|n| u32::try_from(n).ok());
                e
            }
            "PROVIDER_RATE_LIMITED" => made(
                ErrorCode::RateLimit,
                "Your Revoye hourly cap for this provider is used up. Wait, or pin another provider.".to_owned(),
            ),
            "NO_DEVICE_ONLINE" => made(ErrorCode::Server, "No Revoye device is online. Start Revoye Desk on your machine.".to_owned()),
            "NO_AGENT_AVAILABLE" => made(
                ErrorCode::Server,
                "No Revoye agent is free: every agent is busy, disabled or rate-limited.".to_owned(),
            ),
            "JOB_TIMEOUT" => made(ErrorCode::Server, "The Revoye job is still running and was not abandoned.".to_owned()),
            "JOB_FAILED" => made(
                ErrorCode::Server,
                format!("The Revoye job failed after {} attempt(s).", detail_u32("attempts").unwrap_or(1)),
            ),
            "JOB_CANCELLED" => made(ErrorCode::Server, "The Revoye job was cancelled.".to_owned()),
            "NOT_FOUND" => made(ErrorCode::Server, "The Revoye job was not found.".to_owned()),
            "INTERNAL_ERROR" => made(ErrorCode::Server, "Revoye had an internal error. Try again.".to_owned()),
            "PAYLOAD_TOO_LARGE" => made(
                ErrorCode::BadRequest,
                format!("The prompt is too large for Revoye (limit {}).", detail_u32("limit").unwrap_or(100_000)),
            ),
            "CONFLICT" => made(ErrorCode::BadRequest, "The Revoye idempotency key was reused with a different request.".to_owned()),
            "INVALID_REQUEST" => made(ErrorCode::BadRequest, "Revoye rejected the request as invalid.".to_owned()),
            _ => {
                let mut e = error_from_status(status, format!("Revoye request failed ({status})."), KIND);
                // On Revoye a 404 is a missing job, never a missing model.
                if status == 404 {
                    e.code = ErrorCode::Server;
                }
                e
            }
        }
    }

    fn list_models(&self) -> Option<&dyn ListModels> {
        Some(self)
    }

    fn poll(&self) -> Option<&dyn Poll> {
        Some(self)
    }

    fn fleet_status(&self) -> Option<&dyn FleetStatus> {
        Some(self)
    }
}

impl ListModels for Revoye {
    fn build_request(&self, _base_url: Option<&str>, api_key: Option<&str>) -> HttpRequest {
        HttpRequest::get(format!("{BASE_URL}/v1/models"), auth(api_key.unwrap_or_default()))
    }

    fn parse_response(&self, json: &Value) -> ModelList {
        let mut kinds: Vec<String> = Vec::new();
        for id in json
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| m.get("id")?.as_str())
        {
            if id != AUTO && !kinds.iter().any(|k| k == id) {
                kinds.push(id.to_owned());
            }
        }
        let models = if kinds.is_empty() {
            Vec::new()
        } else {
            std::iter::once(AUTO.to_owned()).chain(kinds).collect()
        };
        ModelList { models, note: str_at(json, "/_revoye/note").map(str::to_owned) }
    }
}

impl Poll for Revoye {
    fn interval_ms(&self) -> u64 {
        4_000
    }

    fn max_ms(&self) -> u64 {
        JOB_DEADLINE_MS
    }

    fn build_request(&self, api_key: &str, job_id: &str) -> HttpRequest {
        HttpRequest::get(format!("{BASE_URL}/v1/completions/{}", encode(job_id)), auth(api_key))
    }

    fn parse_response(&self, json: &Value) -> Result<ParseOutcome, ProviderError> {
        parse_job(json)
    }

    fn build_cancel(&self, api_key: &str, job_id: &str) -> HttpRequest {
        HttpRequest {
            method: Method::Delete,
            url: format!("{BASE_URL}/v1/completions/{}", encode(job_id)),
            headers: auth(api_key),
            body: None,
        }
    }
}

impl FleetStatus for Revoye {
    fn build_request(&self, api_key: &str) -> HttpRequest {
        HttpRequest::get(format!("{BASE_URL}/v1/status"), auth(api_key))
    }

    fn parse_response(&self, json: &Value) -> Fleet {
        let n = |p: &str| u32_at(json, p).unwrap_or(0);
        Fleet {
            devices_total: n("/devices/total"),
            devices_online: n("/devices/online"),
            agents_total: n("/agents/total"),
            agents_idle: n("/agents/idle"),
            queue_depth: n("/queue/depth"),
            providers: json
                .get("providers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|p| FleetProvider {
                    kind: str_at(p, "/kind").unwrap_or_default().to_owned(),
                    enabled: p.get("enabled").and_then(Value::as_bool).unwrap_or(false),
                    agents_idle: u32_at(p, "/agents_idle").unwrap_or(0),
                })
                .collect(),
        }
    }
}

/// Percent-encodes a path segment (job ids are `job_…`, but never trust input).
fn encode(segment: &str) -> String {
    segment
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
