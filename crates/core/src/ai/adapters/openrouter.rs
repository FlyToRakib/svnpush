//! OpenRouter: the stock chat dialect plus attribution headers, errors that
//! arrive inside a 200, and the live `GET /models` catalogue.

use serde_json::Value;

use crate::ai::provider::{
    Adapter, AdapterMeta, ErrorCode, FinishReason, GenerateRequest, HttpRequest, ListModels,
    ModelList, ParseOutcome, ProviderError, error_from_status, str_at, trim_base,
};

use super::openai::{OpenAiCompatible, TokenField};

const KIND: &str = "openrouter";
const BASE_URL: &str = "https://openrouter.ai/api/v1";
const DEFAULT_MODEL: &str = "anthropic/claude-sonnet-5";
/// Identifies SVNpush, never the developer.
const APP_URL: &str = "https://github.com/FlyToRakib/svnpush";
const APP_TITLE: &str = "SVNpush";

static META: AdapterMeta = AdapterMeta {
    kind: KIND,
    label: "OpenRouter",
    recommended: false,
    default_base_url: BASE_URL,
    fixed_base_url: false,
    default_model: DEFAULT_MODEL,
    models: &[DEFAULT_MODEL],
    keyless: false,
    key_placeholder: "sk-or-v1-…",
    note: "One key for every major model, billed to your OpenRouter credits.",
    model_hint: "Model ids are author/slug, for example anthropic/claude-sonnet-5. Use ↻ to load the live catalogue.",
};

/// The OpenRouter adapter.
pub struct OpenRouter {
    base: OpenAiCompatible,
}

/// The OpenRouter adapter instance.
pub static OPENROUTER: OpenRouter =
    OpenRouter { base: OpenAiCompatible::new(META, TokenField::MaxTokens) };

/// An OpenRouter `{ code, message, metadata }` error object → a failure.
fn from_error_object(status: u16, error: &Value) -> ProviderError {
    let mut message = str_at(error, "/message")
        .map_or_else(|| format!("OpenRouter request failed ({status})."), str::to_owned);
    if let Some(upstream) =
        str_at(error, "/metadata/provider_name").filter(|p| !message.contains(p))
    {
        message = format!("{message} (via {upstream})");
    }
    let mut e = error_from_status(status, message, KIND);
    match status {
        400 if e.message.to_ascii_lowercase().contains("not a valid model") => {
            e.code = ErrorCode::ModelNotFound;
        }
        403 if error.pointer("/metadata/reasons").is_some()
            || error.pointer("/metadata/flagged_input").is_some() =>
        {
            e.code = ErrorCode::Safety;
        }
        408 => e.code = ErrorCode::Server,
        _ => {}
    }
    e.retry_after = error
        .pointer("/metadata/retry_after")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .and_then(|n| u32::try_from(n).ok());
    e
}

impl Adapter for OpenRouter {
    fn meta(&self) -> &AdapterMeta {
        &META
    }

    fn build_request(&self, req: &GenerateRequest<'_>) -> Result<HttpRequest, ProviderError> {
        let mut request = self.base.chat_request(req);
        request.headers.push(("HTTP-Referer".to_owned(), APP_URL.to_owned()));
        request.headers.push(("X-OpenRouter-Title".to_owned(), APP_TITLE.to_owned()));
        Ok(request)
    }

    fn parse_response(&self, json: &Value) -> Result<ParseOutcome, ProviderError> {
        if let Some(error) = json.get("error").or_else(|| json.pointer("/choices/0/error")) {
            let status = error
                .get("code")
                .and_then(Value::as_u64)
                .and_then(|c| u16::try_from(c).ok())
                .unwrap_or(502);
            return Err(from_error_object(status, error));
        }
        let result = OpenAiCompatible::chat_response(json);
        match &result.finish_reason {
            FinishReason::Other(reason) if reason == "error" => Err(ProviderError::new(
                ErrorCode::Server,
                KIND,
                "The upstream model failed mid-response. Try again or pick another model.",
            )
            .with_status(502)),
            FinishReason::Safety if result.text.is_empty() => {
                Err(ProviderError::new(ErrorCode::Safety, KIND, "The model declined this request.")
                    .with_status(200))
            }
            _ => Ok(ParseOutcome::Complete(result)),
        }
    }

    fn parse_error(&self, status: u16, json: &Value) -> ProviderError {
        from_error_object(status, json.get("error").unwrap_or(json))
    }

    fn list_models(&self) -> Option<&dyn ListModels> {
        Some(self)
    }
}

impl ListModels for OpenRouter {
    fn build_request(&self, base_url: Option<&str>, api_key: Option<&str>) -> HttpRequest {
        let base = base_url.filter(|b| !b.trim().is_empty()).unwrap_or(BASE_URL);
        let headers = api_key
            .filter(|k| !k.is_empty())
            .map(|k| ("authorization".to_owned(), format!("Bearer {k}")))
            .into_iter()
            .collect();
        HttpRequest::get(format!("{}/models", trim_base(base)), headers)
    }

    fn parse_response(&self, json: &Value) -> ModelList {
        let mut models: Vec<String> = json
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|m| {
                m.pointer("/architecture/output_modalities")
                    .and_then(Value::as_array)
                    .is_none_or(|mods| mods.iter().any(|x| x.as_str() == Some("text")))
            })
            .filter_map(|m| m.get("id")?.as_str().map(str::to_owned))
            .collect();
        models.sort();
        let note = models
            .is_empty()
            .then(|| "OpenRouter returned no models. Try ↻ again in a moment.".to_owned());
        ModelList { models, note }
    }
}
