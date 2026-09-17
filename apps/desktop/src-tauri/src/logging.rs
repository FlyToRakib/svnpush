//! Rotated, redacted log files in `<app-data>/svnpush/logs/` (plan §14.1).

use std::io::Write;
use std::path::{Path, PathBuf};

use svnpush_core::secret::redact_secrets;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};

/// Daily log files kept.
const KEPT_FILES: usize = 7;

/// Redacts anything shaped like a key before bytes reach the file.
struct RedactingWriter<W: Write>(W);

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        self.0.write_all(redact_secrets(&text).as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// Starts logging to daily files. Keep the guard alive for the app's lifetime.
pub fn init(dir: &Path) -> Option<WorkerGuard> {
    std::fs::create_dir_all(dir).ok()?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("svnpush")
        .filename_suffix("log")
        .max_log_files(KEPT_FILES)
        .build(dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(move || RedactingWriter(writer.clone()))
        .try_init()
        .ok()?;
    Some(guard)
}

/// The newest log file.
fn newest(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

/// The last `lines` lines of the newest log, redacted again for safety.
pub fn tail(dir: &Path, lines: usize) -> String {
    let Some(text) = newest(dir).and_then(|p| std::fs::read_to_string(p).ok()) else {
        return String::new();
    };
    let all: Vec<&str> = text.lines().collect();
    redact_secrets(&all[all.len().saturating_sub(lines)..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_redacts_and_tail_reads_the_newest_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut out = Vec::new();
        RedactingWriter(&mut out).write_all(b"token Bearer abcdefghijkl1234 end\n").unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "token [redacted] end\n");

        std::fs::write(dir.path().join("svnpush.2026-09-16.log"), "old\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.path().join("svnpush.2026-09-17.log"), "a\nb\nc\n").unwrap();
        assert_eq!(tail(dir.path(), 2), "b\nc");
    }
}
