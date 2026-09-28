//! Asking for JSON: validate the answer, retry once with the validation error,
//! then give up with the raw text so the developer can see it (plan §9.7).

use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::error::Coded;

use super::client::ClientError;
use super::prompts::truncate_chars;
use super::provider::FinishReason;
use super::records::{ProviderRecord, ProvidersFile};
use super::router::{Ask, RouteEvent, Routed, Router};

/// Why a JSON task produced no usable answer.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TaskError {
    /// The provider failed or the request was cancelled.
    #[error(transparent)]
    Client(#[from] ClientError),
    /// Two answers in a row did not match the schema.
    #[error("The AI's answer could not be used: {reason}")]
    Invalid {
        /// The last validation error.
        reason: String,
        /// The last answer, shown to the developer as text.
        raw_text: String,
        /// Who answered.
        provider: Box<ProviderRecord>,
    },
    /// The answer stopped at the output limit.
    #[error("The AI's answer was cut off at the output limit.")]
    Truncated {
        /// The partial answer.
        raw_text: String,
        /// Who answered.
        provider: Box<ProviderRecord>,
    },
    /// The model refused.
    #[error("The model declined to write this.")]
    Refused {
        /// Who answered.
        provider: Box<ProviderRecord>,
    },
}

impl Coded for TaskError {
    fn code(&self) -> &'static str {
        match self {
            Self::Client(e) => e.code(),
            Self::Invalid { .. } => "AI_INVALID_ANSWER",
            Self::Truncated { .. } => "AI_TRUNCATED",
            Self::Refused { .. } => "AI_SAFETY",
        }
    }

    fn fix(&self) -> Option<String> {
        match self {
            Self::Client(e) => e.fix(),
            Self::Invalid { .. } => {
                Some("Write it by hand, regenerate, or choose another provider.".to_owned())
            }
            Self::Truncated { .. } => {
                Some("Regenerate, or choose a model with a larger output limit.".to_owned())
            }
            Self::Refused { .. } => Some("Write it by hand.".to_owned()),
        }
    }
}

/// A validated answer and who gave it.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer<T> {
    /// The parsed JSON.
    pub value: T,
    /// Routing details.
    pub routed: Routed,
}

/// Output ceiling for JSON tasks. Thinking models count their reasoning
/// against it, so it is well above what the answer itself needs.
pub const JSON_MAX_TOKENS: u32 = 16_000;

/// Characters of the validation error sent back with the retry.
const RETRY_REASON_CHARS: usize = 2_000;

/// Asks `ask`, validates against `schema`, and retries once with the error.
pub async fn ask_json<T: DeserializeOwned>(
    router: &Router<'_>,
    file: &ProvidersFile,
    first: &ProviderRecord,
    ask: &Ask<'_>,
    schema: &Value,
    cancel: &CancellationToken,
    on_event: &(dyn Fn(RouteEvent) + Send + Sync),
) -> Result<Answer<T>, TaskError> {
    let answered = router.generate(file, first, ask, cancel, on_event).await?;
    let first_error = match check(&answered, schema) {
        Ok(value) => return Ok(Answer { value, routed: answered }),
        Err(stop @ (TaskError::Refused { .. } | TaskError::Truncated { .. })) => return Err(stop),
        Err(TaskError::Invalid { reason, .. }) => reason,
        Err(other) => return Err(other),
    };
    let retry_user = retry_user(ask.user, &first_error);
    let retry = Ask { user: &retry_user, ..*ask };
    let retried = router.generate(file, &answered.provider, &retry, cancel, on_event).await?;
    check(&retried, schema).map(|value| Answer { value, routed: retried })
}

/// The prompt for the one retry. The reason is capped: a schema error can
/// quote the whole wrong value, which would push the retry past a
/// provider's prompt limit (Revoye's 100,000 characters).
fn retry_user(user: &str, reason: &str) -> String {
    let reason = truncate_chars(reason, RETRY_REASON_CHARS);
    format!(
        "{user}\n\nYour previous answer could not be used. {reason} Reply again with only the JSON object."
    )
}

fn check<T: DeserializeOwned>(routed: &Routed, schema: &Value) -> Result<T, TaskError> {
    let provider = || Box::new(routed.provider.clone());
    match routed.result.finish_reason {
        FinishReason::Safety if routed.result.text.trim().is_empty() => {
            return Err(TaskError::Refused { provider: provider() });
        }
        FinishReason::Length => {
            return Err(TaskError::Truncated {
                raw_text: routed.result.text.clone(),
                provider: provider(),
            });
        }
        _ => {}
    }
    super::schemas::parse_answer(&routed.result.text, schema).map_err(|reason| TaskError::Invalid {
        reason,
        raw_text: routed.result.text.clone(),
        provider: provider(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_retry_prompt_carries_at_most_a_capped_reason() {
        let reason =
            format!("The JSON did not match the schema: {} at /summary.", "é".repeat(50_000));
        let prompt = retry_user("Ask.", &reason);
        assert!(prompt.starts_with("Ask.\n\nYour previous answer could not be used. The JSON"));
        assert!(prompt.ends_with("Reply again with only the JSON object."));
        assert!(prompt.chars().count() < 2_200, "{}", prompt.chars().count());
        assert_eq!(
            retry_user("Ask.", "Short."),
            "Ask.\n\nYour previous answer could not be used. Short. Reply again with only the JSON object."
        );
    }
}
