//! Running `svn` and `git`: streamed, redacted, cancellable.

use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio_util::sync::CancellationToken;

use crate::report::{LogLine, LogStream, Reporter};
use crate::secret::Secret;

/// What to run.
#[derive(Debug)]
pub struct ProcessSpec<'a> {
    /// The executable.
    pub program: &'a Path,
    /// Arguments. Never put a secret here: use `stdin`.
    pub args: Vec<OsString>,
    /// Working directory.
    pub cwd: Option<&'a Path>,
    /// Written to standard input, then the handle is closed.
    pub stdin: Option<&'a Secret>,
    /// Whether output lines go to the log drawer (off for file contents).
    pub log_output: bool,
}

/// A finished process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    /// Exit code; `None` when killed by a signal.
    pub code: Option<i32>,
    /// Standard output, lossily decoded.
    pub stdout: String,
    /// Standard error, lossily decoded.
    pub stderr: String,
}

impl ProcessOutput {
    /// Whether the process exited with code 0.
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Why a process could not run to completion.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    /// The executable could not be started.
    #[error("could not start {program}: {source}")]
    Spawn { program: String, source: std::io::Error },
    /// Reading output or writing input failed.
    #[error("{program} I/O failed: {source}")]
    Io { program: String, source: std::io::Error },
    /// The operation was cancelled and the process killed.
    #[error("cancelled")]
    Cancelled,
}

/// Windows: do not flash a console window for each child process.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn quote(arg: &OsString) -> String {
    let text = arg.to_string_lossy();
    if text.contains(' ') { format!("\"{text}\"") } else { text.into_owned() }
}

async fn collect<R: AsyncRead + Unpin>(
    reader: R,
    stream: LogStream,
    log: bool,
    secret: Option<&Secret>,
    reporter: &dyn Reporter,
) -> std::io::Result<String> {
    let mut reader = BufReader::new(reader);
    let mut all = String::new();
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        if reader.read_until(b'\n', &mut buffer).await? == 0 {
            break;
        }
        let line = String::from_utf8_lossy(&buffer);
        let line = secret.map_or_else(|| line.to_string(), |s| s.redact(&line));
        if log {
            let text = line.trim_end_matches(['\r', '\n']);
            if !text.is_empty() {
                reporter.log(LogLine { stream, text: text.to_owned() });
            }
        }
        all.push_str(&line);
    }
    Ok(all)
}

/// Runs a process to completion, streaming its output to `reporter`.
///
/// Messages are requested in English (`LC_MESSAGES=C`) so they read the same
/// in every log; the character set is left alone so non-ASCII file names work.
pub async fn run(
    spec: ProcessSpec<'_>,
    reporter: &dyn Reporter,
    cancel: &CancellationToken,
) -> Result<ProcessOutput, ProcessError> {
    let program = spec.program.display().to_string();
    let file_name = spec.program.file_name().map_or_else(
        || program.clone(),
        |n| n.to_string_lossy().trim_end_matches(".exe").to_owned(),
    );
    let shown: Vec<String> = spec.args.iter().map(quote).collect();
    reporter.log(LogLine {
        stream: LogStream::Command,
        text: format!("{file_name} {}", shown.join(" ")),
    });

    let mut command = tokio::process::Command::new(spec.program);
    command
        .args(&spec.args)
        .env_remove("LC_ALL")
        .env("LC_MESSAGES", "C")
        .env("LANGUAGE", "C")
        .stdin(if spec.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = spec.cwd {
        command.current_dir(cwd);
    }
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = command
        .spawn()
        .map_err(|source| ProcessError::Spawn { program: program.clone(), source })?;

    if let (Some(secret), Some(mut stdin)) = (spec.stdin, child.stdin.take()) {
        stdin
            .write_all(secret.expose().as_bytes())
            .await
            .map_err(|source| ProcessError::Io { program: program.clone(), source })?;
        drop(stdin);
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let io = |source| ProcessError::Io { program: program.clone(), source };

    let work = async {
        let out = async {
            match stdout {
                Some(s) => {
                    collect(s, LogStream::Stdout, spec.log_output, spec.stdin, reporter).await
                }
                None => Ok(String::new()),
            }
        };
        let err = async {
            match stderr {
                Some(s) => collect(s, LogStream::Stderr, true, spec.stdin, reporter).await,
                None => Ok(String::new()),
            }
        };
        let (out, err) = tokio::join!(out, err);
        let status = child.wait().await;
        (out, err, status)
    };

    tokio::select! {
        () = cancel.cancelled() => Err(ProcessError::Cancelled),
        (out, err, status) = work => {
            let status = status.map_err(io)?;
            Ok(ProcessOutput { code: status.code(), stdout: out.map_err(io)?, stderr: err.map_err(io)? })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<LogLine>>);

    impl Reporter for Recorder {
        fn log(&self, line: LogLine) {
            self.0.lock().unwrap().push(line);
        }
    }

    #[tokio::test]
    async fn output_lines_are_redacted_before_logging_and_returning() {
        let recorder = Recorder::default();
        let secret = Secret::new("s3cret-pw");
        let input: &[u8] = b"Authentication with s3cret-pw failed\r\nsecond line\n";
        let all = collect(input, LogStream::Stderr, true, Some(&secret), &recorder).await.unwrap();
        assert!(!all.contains("s3cret-pw"));
        let lines = recorder.0.lock().unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Authentication with [redacted] failed");
        assert_eq!(lines[1].stream, LogStream::Stderr);
    }

    #[tokio::test]
    async fn a_missing_program_is_a_spawn_error() {
        let spec = ProcessSpec {
            program: Path::new("definitely-not-a-program-svnpush"),
            args: Vec::new(),
            cwd: None,
            stdin: None,
            log_output: true,
        };
        let err = run(spec, &Recorder::default(), &CancellationToken::new()).await.unwrap_err();
        assert!(matches!(err, ProcessError::Spawn { .. }));
    }
}
