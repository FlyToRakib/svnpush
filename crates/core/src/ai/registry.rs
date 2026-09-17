//! Every adapter, looked up by kind (plan §9.2).

use super::adapters::{claude, gemini, openai, openrouter, revoye};
use super::order;
use super::provider::{Adapter, ErrorCode, ProviderError};

/// Registration order after Revoye (plan §9.2 table).
static ADAPTERS: [&dyn Adapter; 10] = [
    &revoye::REVOYE,
    &gemini::GEMINI,
    &claude::CLAUDE,
    &openai::OPENAI,
    &openrouter::OPENROUTER,
    &openai::DEEPSEEK,
    &openai::QWEN,
    &openai::PERPLEXITY,
    &openai::OPENAI_COMPATIBLE,
    &openai::LOCAL,
];

/// The adapter for `kind`.
pub fn get(kind: &str) -> Result<&'static dyn Adapter, ProviderError> {
    ADAPTERS.iter().copied().find(|a| a.meta().kind == kind).ok_or_else(|| ProviderError {
        code: ErrorCode::NoProvider,
        status: 0,
        retry_after: None,
        provider_kind: None,
        message: format!("\"{kind}\" is not a provider SVNpush knows."),
    })
}

/// Every adapter, Revoye first.
pub fn list() -> Vec<&'static dyn Adapter> {
    order::revoye_first(ADAPTERS.to_vec(), |a| a.meta().kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revoye_first_then_registration_order() {
        let kinds: Vec<&str> = list().iter().map(|a| a.meta().kind).collect();
        assert_eq!(
            kinds,
            [
                "revoye",
                "gemini",
                "claude",
                "openai",
                "openrouter",
                "deepseek",
                "qwen",
                "perplexity",
                "openai_compatible",
                "local"
            ]
        );
    }

    #[test]
    fn only_revoye_is_recommended_and_fixed() {
        for adapter in list() {
            let meta = adapter.meta();
            assert_eq!(meta.recommended, meta.kind == "revoye", "{}", meta.kind);
            assert_eq!(meta.fixed_base_url, meta.kind == "revoye", "{}", meta.kind);
            assert_eq!(meta.keyless, meta.kind == "local", "{}", meta.kind);
        }
    }

    #[test]
    fn unknown_kind_is_no_provider() {
        assert_eq!(get("nope").map(|a| a.meta().kind).unwrap_err().code, ErrorCode::NoProvider);
        assert!(get("claude").is_ok());
    }
}
