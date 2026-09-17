//! Anthropic Claude, Messages API. Sampling parameters are never sent: current
//! models reject them. JSON answers use `output_config.format`.

use serde_json::{Value, json};

use crate::ai::provider::{
    Adapter, AdapterMeta, ErrorCode, FinishReason, GenerateRequest, GenerateResult, HttpRequest,
    ParseOutcome, ProviderError, error_from_status, str_at, trim_base, u32_at,
};

use super::openai::DEFAULT_MAX_TOKENS;

const KIND: &str = "claude";
const API_VERSION: &str = "2023-06-01";

static META: AdapterMeta = AdapterMeta {
    kind: KIND,
    label: "Anthropic Claude",
    recommended: false,
    default_base_url: "https://api.anthropic.com",
    fixed_base_url: false,
    default_model: "claude-sonnet-5",
    models: &[],
    keyless: false,
    key_placeholder: "sk-ant-…",
    note: "Requests are billed to your Anthropic Console account.",
    model_hint: "Default: claude-sonnet-5. Enter any Claude model id.",
};

/// The Claude adapter.
pub struct Claude;

/// The Claude adapter instance.
pub static CLAUDE: Claude = Claude;

impl Adapter for Claude {
    fn meta(&self) -> &AdapterMeta {
        &META
    }

    fn build_request(&self, req: &GenerateRequest<'_>) -> Result<HttpRequest, ProviderError> {
        let api_key = req
            .api_key
            .ok_or_else(|| ProviderError::new(ErrorCode::Auth, KIND, "Claude needs an API key."))?;
        let messages: Vec<Value> =
            req.messages.iter().map(|m| json!({ "role": m.role, "content": m.content })).collect();
        let mut body = json!({
            "model": req.model,
            "max_tokens": req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            "messages": messages,
        });
        if let Some(system) = req.system.filter(|s| !s.is_empty()) {
            body["system"] = json!(system);
        }
        if let Some(schema) = req.json_schema {
            body["output_config"] =
                json!({ "format": { "type": "json_schema", "schema": schema } });
        }
        let base = req.base_url.filter(|b| !b.trim().is_empty()).unwrap_or(META.default_base_url);
        Ok(HttpRequest::post(
            format!("{}/v1/messages", trim_base(base)),
            vec![
                ("content-type".to_owned(), "application/json".to_owned()),
                ("x-api-key".to_owned(), api_key.to_owned()),
                ("anthropic-version".to_owned(), API_VERSION.to_owned()),
            ],
            body,
        ))
    }

    fn parse_response(&self, json: &Value) -> Result<ParseOutcome, ProviderError> {
        let stop = str_at(json, "/stop_reason").unwrap_or("end_turn");
        if stop == "refusal" {
            return Err(ProviderError::new(
                ErrorCode::Safety,
                KIND,
                "The model declined this request.",
            )
            .with_status(200));
        }
        let text: String = json
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|b| str_at(b, "/type") == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect();
        let finish = match stop {
            "end_turn" | "stop_sequence" => FinishReason::Stop,
            "max_tokens" => FinishReason::Length,
            other => FinishReason::Other(other.to_owned()),
        };
        Ok(ParseOutcome::Complete(GenerateResult {
            text,
            tokens_in: u32_at(json, "/usage/input_tokens"),
            tokens_out: u32_at(json, "/usage/output_tokens"),
            finish_reason: finish,
            raw: json.clone(),
        }))
    }

    fn parse_error(&self, status: u16, json: &Value) -> ProviderError {
        let message = str_at(json, "/error/message")
            .map_or_else(|| format!("Anthropic request failed ({status})."), str::to_owned);
        let mut e = error_from_status(status, message, KIND);
        match str_at(json, "/error/type") {
            Some("overloaded_error") => e.code = ErrorCode::Server,
            Some("billing_error") => e.code = ErrorCode::Payment,
            Some("request_too_large") => e.code = ErrorCode::BadRequest,
            _ => {}
        }
        e
    }
}
