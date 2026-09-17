//! A value that must never be printed, logged or serialised, and the
//! defensive redaction applied to every log line.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;

/// Shapes of API keys and bearer tokens the log redaction removes.
static SECRET_SHAPES: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)(bearer\s+[A-Za-z0-9._~+/=-]{8,}",
        r"|revoye_sk_(?:live|test)_[A-Za-z0-9]+",
        r"|sk-ant-[A-Za-z0-9_-]{8,}",
        r"|sk-or-v1-[A-Za-z0-9]{8,}",
        r"|sk-[A-Za-z0-9_-]{16,}",
        r"|AIza[0-9A-Za-z_-]{20,}",
        r"|(?:x-api-key|x-goog-api-key|authorization)\s*[:=]\s*\S+)"
    ))
    .ok()
});

/// Replaces anything shaped like an API key or bearer token with `[redacted]`.
/// A defence in depth: secrets are never passed to the logger in the first place.
pub fn redact_secrets(text: &str) -> String {
    SECRET_SHAPES
        .as_ref()
        .map_or_else(|| text.to_owned(), |re| re.replace_all(text, "[redacted]").into_owned())
}

/// A password or API key. `Debug` and `Display` print a placeholder, and the
/// type deliberately implements neither `Serialize` nor `Clone`.
pub struct Secret(String);

impl Secret {
    /// Wraps a secret value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The plaintext, for the one place that must send it.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the secret is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Replaces every occurrence of the secret in `text` with `[redacted]`.
    pub fn redact(&self, text: &str) -> String {
        if self.0.is_empty() { text.to_owned() } else { text.replace(&self.0, "[redacted]") }
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_prints_the_value() {
        let s = Secret::new("hunter2");
        assert_eq!(format!("{s:?}"), "Secret([redacted])");
        assert_eq!(format!("{s}"), "[redacted]");
        assert_eq!(s.expose(), "hunter2");
    }

    #[test]
    fn redacts_key_shapes_in_free_text() {
        assert!(SECRET_SHAPES.is_some());
        let text = "auth Bearer revoye_sk_live_3xQ8vP2mK9wR7tY4nL6jH1sD5fG0aZbC and x-api-key: sk-ant-abc123456789 key=AIzaSyA1234567890abcdefghijk ok";
        let out = redact_secrets(text);
        assert!(!out.contains("revoye_sk_live"));
        assert!(!out.contains("sk-ant-abc"));
        assert!(!out.contains("AIzaSy"));
        assert!(out.ends_with(" ok"));
        assert_eq!(redact_secrets("svn commit trunk -m Release"), "svn commit trunk -m Release");
    }

    #[test]
    fn redacts_occurrences() {
        let s = Secret::new("pw123");
        assert_eq!(s.redact("login pw123 failed pw123"), "login [redacted] failed [redacted]");
        assert_eq!(Secret::new("").redact("text"), "text");
    }
}
