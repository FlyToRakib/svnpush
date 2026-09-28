//! The `/chat/completions` dialect: the `OpenAiCompatible` factory behind
//! OpenAI, DeepSeek, Qwen, Perplexity, any compatible endpoint and local
//! servers (plan §9.2).

use serde_json::{Value, json};

use crate::ai::provider::{
    Adapter, AdapterMeta, ErrorCode, FinishReason, GenerateRequest, GenerateResult, HttpRequest,
    ParseOutcome, ProviderError, error_from_status, str_at, trim_base, u32_at,
};

/// Which field carries the output token ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenField {
    /// OpenAI deprecated `max_tokens` in favour of `max_completion_tokens`.
    MaxCompletionTokens,
    /// Compatible servers still read `max_tokens`.
    MaxTokens,
}

/// Default output ceiling when a request sets none.
pub const DEFAULT_MAX_TOKENS: u32 = 8_192;

/// A provider speaking `/chat/completions`.
pub struct OpenAiCompatible {
    meta: AdapterMeta,
    token_field: TokenField,
    output_cap: Option<u32>,
    renamed: &'static [(&'static str, &'static str)],
}

impl OpenAiCompatible {
    /// A factory instance.
    pub const fn new(meta: AdapterMeta, token_field: TokenField) -> Self {
        Self { meta, token_field, output_cap: None, renamed: &[] }
    }

    /// Caps the output ceiling for an API that rejects a larger one.
    #[must_use]
    pub const fn with_output_cap(self, cap: u32) -> Self {
        Self { output_cap: Some(cap), ..self }
    }

    /// Sends the replacement for a model id the provider has retired, so a
    /// record saved with the old id keeps working.
    #[must_use]
    pub const fn with_renamed_models(
        self,
        renamed: &'static [(&'static str, &'static str)],
    ) -> Self {
        Self { renamed, ..self }
    }

    /// The stock request, reused by OpenRouter.
    pub fn chat_request(&self, req: &GenerateRequest<'_>) -> HttpRequest {
        let mut messages = Vec::new();
        if let Some(system) = req.system.filter(|s| !s.is_empty()) {
            messages.push(json!({ "role": "system", "content": system }));
        }
        messages
            .extend(req.messages.iter().map(|m| json!({ "role": m.role, "content": m.content })));
        let model = self
            .renamed
            .iter()
            .find(|(old, _)| *old == req.model)
            .map_or(req.model, |(_, new)| new);
        let mut body = json!({ "model": model, "messages": messages });
        let field = match self.token_field {
            TokenField::MaxCompletionTokens => "max_completion_tokens",
            TokenField::MaxTokens => "max_tokens",
        };
        let max_tokens = req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
        body[field] = json!(self.output_cap.map_or(max_tokens, |cap| max_tokens.min(cap)));
        if let Some(t) = req.temperature {
            body["temperature"] = json!(t);
        }
        if req.json_schema.is_some() {
            body["response_format"] = json!({ "type": "json_object" });
        }
        let mut headers = vec![("content-type".to_owned(), "application/json".to_owned())];
        if let Some(key) = req.api_key.filter(|k| !k.is_empty()) {
            headers.push(("authorization".to_owned(), format!("Bearer {key}")));
        }
        let base =
            req.base_url.filter(|b| !b.trim().is_empty()).unwrap_or(self.meta.default_base_url);
        HttpRequest::post(format!("{}/chat/completions", trim_base(base)), headers, body)
    }

    /// The stock response parse, reused by OpenRouter.
    pub fn chat_response(json: &Value) -> GenerateResult {
        let finish = match str_at(json, "/choices/0/finish_reason").unwrap_or("stop") {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "content_filter" => FinishReason::Safety,
            other => FinishReason::Other(other.to_owned()),
        };
        GenerateResult {
            text: str_at(json, "/choices/0/message/content").unwrap_or_default().to_owned(),
            tokens_in: u32_at(json, "/usage/prompt_tokens"),
            tokens_out: u32_at(json, "/usage/completion_tokens"),
            finish_reason: finish,
            raw: json.clone(),
        }
    }
}

impl Adapter for OpenAiCompatible {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    fn build_request(&self, req: &GenerateRequest<'_>) -> Result<HttpRequest, ProviderError> {
        let base =
            req.base_url.filter(|b| !b.trim().is_empty()).unwrap_or(self.meta.default_base_url);
        if base.trim().is_empty() {
            return Err(ProviderError::new(
                ErrorCode::BadRequest,
                self.meta.kind,
                "This provider needs a base URL. Set it under Advanced on the Providers screen.",
            ));
        }
        Ok(self.chat_request(req))
    }

    fn parse_response(&self, json: &Value) -> Result<ParseOutcome, ProviderError> {
        Ok(ParseOutcome::Complete(Self::chat_response(json)))
    }

    fn parse_error(&self, status: u16, json: &Value) -> ProviderError {
        let message =
            str_at(json, "/error/message").or_else(|| str_at(json, "/message")).map_or_else(
                || format!("{} request failed ({status}).", self.meta.label),
                str::to_owned,
            );
        let mut e = error_from_status(status, message, self.meta.kind);
        // OpenAI reports an exhausted balance as 429 `insufficient_quota`, not 402.
        if str_at(json, "/error/code") == Some("insufficient_quota")
            || str_at(json, "/error/type") == Some("insufficient_quota")
        {
            e.code = ErrorCode::Payment;
        }
        e
    }
}

// One argument per metadata field keeps each factory instance a readable table row.
#[allow(clippy::too_many_arguments)]
const fn meta(
    kind: &'static str,
    label: &'static str,
    base: &'static str,
    model: &'static str,
    keyless: bool,
    key_placeholder: &'static str,
    note: &'static str,
    model_hint: &'static str,
) -> AdapterMeta {
    AdapterMeta {
        kind,
        label,
        recommended: false,
        default_base_url: base,
        fixed_base_url: false,
        default_model: model,
        models: &[],
        keyless,
        key_placeholder,
        note,
        model_hint,
    }
}

/// OpenAI.
pub static OPENAI: OpenAiCompatible = OpenAiCompatible::new(
    meta(
        "openai",
        "OpenAI",
        "https://api.openai.com/v1",
        "gpt-6-astra",
        false,
        "sk-…",
        "Requests are billed to your OpenAI account.",
        "Default: gpt-6-astra. Enter any chat model id your account can use.",
    ),
    TokenField::MaxCompletionTokens,
);

/// DeepSeek. `deepseek-chat` and `deepseek-reasoner` stopped resolving on
/// 2026-07-24; saved records that name them are sent `deepseek-flash`.
pub static DEEPSEEK: OpenAiCompatible = OpenAiCompatible::new(
    meta(
        "deepseek",
        "DeepSeek",
        "https://api.deepseek.com/v1",
        "deepseek-flash",
        false,
        "sk-…",
        "Requests are billed to your DeepSeek account.",
        "Default: deepseek-flash.",
    ),
    TokenField::MaxTokens,
)
.with_renamed_models(&[
    ("deepseek-chat", "deepseek-flash"),
    ("deepseek-reasoner", "deepseek-flash"),
]);

/// Qwen through DashScope's compatible mode. `qwen-plus` rejects more than
/// 8,192 output tokens.
pub static QWEN: OpenAiCompatible = OpenAiCompatible::new(
    meta(
        "qwen",
        "Qwen (DashScope)",
        "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "qwen-plus",
        false,
        "sk-…",
        "Requests are billed to your Alibaba Cloud DashScope account.",
        "Default: qwen-plus.",
    ),
    TokenField::MaxTokens,
)
.with_output_cap(8_192);

/// Perplexity through its Router API. The Sonar chat completions API and its
/// `sonar*` models were retired on 2026-09-27; saved records that name one are
/// sent the Router default.
pub static PERPLEXITY: OpenAiCompatible = OpenAiCompatible::new(
    meta(
        "perplexity",
        "Perplexity",
        "https://api.perplexity.ai/router/v1",
        "perplexity/kimi-k3",
        false,
        "pplx-…",
        "Requests are billed to your Perplexity account.",
        "Default: perplexity/kimi-k3. Enter any model id from Perplexity's Router catalogue.",
    ),
    TokenField::MaxTokens,
)
.with_renamed_models(&[
    ("sonar", "perplexity/kimi-k3"),
    ("sonar-pro", "perplexity/kimi-k3"),
    ("sonar-reasoning-pro", "perplexity/kimi-k3"),
    ("sonar-deep-research", "perplexity/kimi-k3"),
]);

/// Any OpenAI-compatible endpoint.
pub static OPENAI_COMPATIBLE: OpenAiCompatible = OpenAiCompatible::new(
    meta(
        "openai_compatible",
        "OpenAI-compatible",
        "",
        "",
        false,
        "Paste the endpoint's API key",
        "Any server that speaks the OpenAI /chat/completions API. Set its base URL under Advanced.",
        "Enter the model id your endpoint serves.",
    ),
    TokenField::MaxTokens,
);

/// Ollama or LM Studio on this machine.
pub static LOCAL: OpenAiCompatible = OpenAiCompatible::new(
    meta(
        "local",
        "Local model (Ollama / LM Studio)",
        "http://localhost:11434/v1",
        "gemma3",
        true,
        "",
        "Runs on your machine with no API key. Nothing leaves your computer. LM Studio listens on http://localhost:1234/v1.",
        "Default: gemma3. Enter a model you have pulled.",
    ),
    TokenField::MaxTokens,
);
