//! How long operations tell the outside world what they are doing.
//!
//! The core never touches the window: it emits typed events through a
//! [`Reporter`], and the shell forwards them to the UI unchanged.

use serde::Serialize;
use ts_rs::TS;

/// Where a log line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum LogStream {
    /// A command about to run, shown before it starts.
    Command,
    /// The command's standard output.
    Stdout,
    /// The command's standard error.
    Stderr,
    /// A message from SVNpush itself.
    Info,
}

/// One line for the log drawer. Never contains a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LogLine {
    /// The origin of the line.
    pub stream: LogStream,
    /// The text, already redacted.
    pub text: String,
}

/// Receives progress from long-running operations.
pub trait Reporter: Send + Sync {
    /// A line for the log drawer.
    fn log(&self, line: LogLine);

    /// A convenience for [`LogStream::Info`] lines.
    fn info(&self, text: &str) {
        self.log(LogLine { stream: LogStream::Info, text: text.to_owned() });
    }
}

/// A reporter that discards everything.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullReporter;

impl Reporter for NullReporter {
    fn log(&self, _line: LogLine) {}
}
