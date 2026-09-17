//! A value that must never be printed, logged or serialised.

use std::fmt;

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
    fn redacts_occurrences() {
        let s = Secret::new("pw123");
        assert_eq!(s.redact("login pw123 failed pw123"), "login [redacted] failed [redacted]");
        assert_eq!(Secret::new("").redact("text"), "text");
    }
}
