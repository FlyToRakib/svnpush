//! Google Gemini, Generative Language API `generateContent`.

use serde_json::{Value, json};

use crate::ai::provider::{
    Adapter, AdapterMeta, ErrorCode, FinishReason, GenerateRequest, GenerateResult, HttpRequest,
    ParseOutcome, ProviderError, Role, error_from_status, str_at, trim_base, u32_at,
};

use super::openai::DEFAULT_MAX_TOKENS;

const KIND: &str = "gemini";

static META: AdapterMeta = AdapterMeta {
    kind: KIND,
    label: "Google Gemini",
    recommended: false,
    default_base_url: "https://generativelanguage.googleapis.com",
    fixed_base_url: false,
    default_model: "gemini-3.8-flash",
    models: &[],
    keyless: false,
    key_placeholder: "AIza…",
    note: "Requests use your Google AI Studio API key.",
    model_hint: "Default: gemini-3.8-flash. Enter any Gemini model id.",
};

/// The Gemini adapter.
pub struct Gemini;

/// The Gemini adapter instance.
pub static GEMINI: Gemini = Gemini;

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

impl Adapter for Gemini {
    fn meta(&self) -> &AdapterMeta {
        &META
    }

    fn build_request(&self, req: &GenerateRequest<'_>) -> Result<HttpRequest, ProviderError> {
        let api_key = req
            .api_key
            .ok_or_else(|| ProviderError::new(ErrorCode::Auth, KIND, "Gemini needs an API key."))?;
        let contents: Vec<Value> = req
            .messages
            .iter()
            .map(|m| {
                let role = if m.role == Role::Assistant { "model" } else { "user" };
                json!({ "role": role, "parts": [{ "text": m.content }] })
            })
            .collect();
        let mut config = json!({ "maxOutputTokens": req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS) });
        if let Some(t) = req.temperature {
            config["temperature"] = json!(t);
        }
        if let Some(schema) = req.json_schema {
            config["responseMimeType"] = json!("application/json");
            config["responseJsonSchema"] = schema.clone();
        }
        let mut body = json!({ "contents": contents, "generationConfig": config });
        if let Some(system) = req.system.filter(|s| !s.is_empty()) {
            body["systemInstruction"] = json!({ "parts": [{ "text": system }] });
        }
        let base = req.base_url.filter(|b| !b.trim().is_empty()).unwrap_or(META.default_base_url);
        Ok(HttpRequest::post(
            format!(
                "{}/v1beta/models/{}:generateContent",
                trim_base(base),
                percent_encode(req.model)
            ),
            vec![
                ("content-type".to_owned(), "application/json".to_owned()),
                ("x-goog-api-key".to_owned(), api_key.to_owned()),
            ],
            body,
        ))
    }

    fn parse_response(&self, json: &Value) -> Result<ParseOutcome, ProviderError> {
        let candidate = json.pointer("/candidates/0");
        if candidate.is_none() && json.pointer("/promptFeedback/blockReason").is_some() {
            return Err(ProviderError::new(
                ErrorCode::Safety,
                KIND,
                "The model declined this request.",
            )
            .with_status(200));
        }
        let text: String = candidate
            .and_then(|c| c.pointer("/content/parts"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|p| p.get("thought").and_then(Value::as_bool) != Some(true))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect();
        let finish = match candidate.and_then(|c| str_at(c, "/finishReason")).unwrap_or("STOP") {
            "STOP" => FinishReason::Stop,
            "MAX_TOKENS" => FinishReason::Length,
            "SAFETY" | "PROHIBITED_CONTENT" | "BLOCKLIST" | "SPII" | "RECITATION" => {
                FinishReason::Safety
            }
            other => FinishReason::Other(other.to_owned()),
        };
        Ok(ParseOutcome::Complete(GenerateResult {
            text,
            tokens_in: u32_at(json, "/usageMetadata/promptTokenCount"),
            tokens_out: u32_at(json, "/usageMetadata/candidatesTokenCount"),
            finish_reason: finish,
            raw: json.clone(),
        }))
    }

    fn parse_error(&self, status: u16, json: &Value) -> ProviderError {
        let message = str_at(json, "/error/message")
            .map_or_else(|| format!("Gemini request failed ({status})."), str::to_owned);
        let mut e = error_from_status(status, message, KIND);
        // Gemini reports a bad key as 400 INVALID_ARGUMENT with reason API_KEY_INVALID.
        let reason_is_key = json
            .pointer("/error/details")
            .and_then(Value::as_array)
            .is_some_and(|d| d.iter().any(|x| str_at(x, "/reason") == Some("API_KEY_INVALID")));
        if reason_is_key {
            e.code = ErrorCode::Auth;
        }
        e
    }
}
