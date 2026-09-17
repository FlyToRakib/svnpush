//! The contract every error that can reach the UI follows.

/// A typed error with a stable machine code and an optional fix.
///
/// Codes are `SCREAMING_SNAKE_CASE`, never change once shipped, and are what
/// the UI branches on. The message is the `Display` text; the fix, when
/// present, tells the developer what to do next.
pub trait Coded: std::error::Error {
    /// The stable code, for example `DETECT_NO_MAIN_FILE`.
    fn code(&self) -> &'static str;

    /// What the developer can do about it, when the engine knows.
    fn fix(&self) -> Option<String> {
        None
    }
}
