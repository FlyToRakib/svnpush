//! Running `svn`, `git` and the pre-build command: streamed, redacted, cancellable.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

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

/// How long output is still read after a process exits. A daemon it started
/// can hold the pipes open forever, and its output is not worth a hung release.
const DRAIN: Duration = Duration::from_secs(2);

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
    all: &mut String,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(reader);
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
    Ok(())
}

/// The `LC_CTYPE` a child needs for non-ASCII file names when the locale it
/// inherits (without `LC_ALL`, which is removed) is not UTF-8. Apps started
/// from the macOS Finder or some Linux desktops get the `C` locale.
#[cfg_attr(not(unix), allow(dead_code))]
fn utf8_ctype(lc_ctype: Option<&str>, lang: Option<&str>) -> Option<&'static str> {
    let effective = lc_ctype.filter(|v| !v.is_empty()).or(lang.filter(|v| !v.is_empty()));
    let utf8 = effective.is_some_and(|v| {
        let lower = v.to_ascii_lowercase();
        lower.contains("utf-8") || lower.contains("utf8")
    });
    if utf8 {
        None
    } else if cfg!(target_os = "macos") {
        Some("en_US.UTF-8")
    } else {
        Some("C.UTF-8")
    }
}

/// Kills the process and everything it started: the process tree on
/// Windows (`taskkill /T`), the process group on Unix (every child is a group
/// leader). A shell's grandchildren, such as `npm` and `node`, die with it.
async fn kill_tree(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    #[cfg(windows)]
    let mut command = {
        let taskkill = std::env::var_os("SystemRoot").map_or_else(
            || PathBuf::from("taskkill.exe"),
            |root| PathBuf::from(root).join("System32").join("taskkill.exe"),
        );
        let mut command = tokio::process::Command::new(taskkill);
        command.args(["/PID", &pid.to_string(), "/T", "/F"]).creation_flags(CREATE_NO_WINDOW);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        // `/bin/kill` on macOS (FreeBSD's, in shell_cmds) skips a `--` after
        // the signal, and util-linux/procps need it before a negative pid.
        let mut command = tokio::process::Command::new("kill");
        command.args(["-KILL", "--", &format!("-{pid}")]);
        command
    };
    let _ = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().await;
}

/// Runs a process to completion, streaming its output to `reporter`.
///
/// Messages are requested in English (`LC_MESSAGES=C`) so they read the same
/// in every log; the character set is left alone (or made UTF-8 when it is
/// not) so non-ASCII file names work.
pub async fn run(
    spec: ProcessSpec<'_>,
    reporter: &dyn Reporter,
    cancel: &CancellationToken,
) -> Result<ProcessOutput, ProcessError> {
    let file_name = spec.program.file_name().map_or_else(
        || spec.program.display().to_string(),
        |n| n.to_string_lossy().trim_end_matches(".exe").to_owned(),
    );
    let shown: Vec<String> = spec.args.iter().map(quote).collect();
    let mut command = tokio::process::Command::new(spec.program);
    command.args(&spec.args);
    execute(command, &spec, format!("{file_name} {}", shown.join(" ")), reporter, cancel).await
}

/// Runs `command_line` through the platform shell (`cmd.exe /D /S /C` or
/// `/bin/sh -c`) in `cwd`. On Windows the line reaches `cmd.exe` verbatim,
/// so quotes inside it work as typed.
pub async fn run_shell(
    command_line: &str,
    cwd: &Path,
    reporter: &dyn Reporter,
    cancel: &CancellationToken,
) -> Result<ProcessOutput, ProcessError> {
    #[cfg(windows)]
    let (program, command) = {
        let comspec =
            std::env::var_os("ComSpec").map_or_else(|| PathBuf::from("cmd.exe"), PathBuf::from);
        let mut command = tokio::process::Command::new(&comspec);
        // With /S, cmd.exe strips exactly the outer pair of quotes.
        command.args(["/D", "/S", "/C"]).raw_arg(format!("\"{command_line}\""));
        (comspec, command)
    };
    #[cfg(not(windows))]
    let (program, command) = {
        let sh = PathBuf::from("/bin/sh");
        let mut command = tokio::process::Command::new(&sh);
        command.arg("-c").arg(command_line);
        (sh, command)
    };
    let spec = ProcessSpec {
        program: &program,
        args: Vec::new(),
        cwd: Some(cwd),
        stdin: None,
        log_output: true,
    };
    execute(command, &spec, command_line.to_owned(), reporter, cancel).await
}

async fn execute(
    mut command: tokio::process::Command,
    spec: &ProcessSpec<'_>,
    shown: String,
    reporter: &dyn Reporter,
    cancel: &CancellationToken,
) -> Result<ProcessOutput, ProcessError> {
    let program = spec.program.display().to_string();
    reporter.log(LogLine { stream: LogStream::Command, text: shown });

    command
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
    #[cfg(unix)]
    {
        let ctype = std::env::var("LC_CTYPE").ok();
        let lang = std::env::var("LANG").ok();
        if let Some(value) = utf8_ctype(ctype.as_deref(), lang.as_deref()) {
            command.env("LC_CTYPE", value);
        }
        command.process_group(0);
    }

    let mut child = command
        .spawn()
        .map_err(|source| ProcessError::Spawn { program: program.clone(), source })?;
    let pid = child.id();

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
    let (mut out_text, mut err_text) = (String::new(), String::new());

    let status = {
        let out = async {
            match stdout {
                Some(s) => {
                    let log = spec.log_output;
                    collect(s, LogStream::Stdout, log, spec.stdin, reporter, &mut out_text).await
                }
                None => Ok(()),
            }
        };
        let err = async {
            match stderr {
                Some(s) => {
                    collect(s, LogStream::Stderr, true, spec.stdin, reporter, &mut err_text).await
                }
                None => Ok(()),
            }
        };
        tokio::pin!(out, err);
        let (mut out_done, mut err_done) = (None, None);
        // Wait for the exit itself, reading output meanwhile so full pipes never stall it.
        let exited = loop {
            tokio::select! {
                () = cancel.cancelled() => break None,
                r = &mut out, if out_done.is_none() => out_done = Some(r),
                r = &mut err, if err_done.is_none() => err_done = Some(r),
                status = child.wait() => break Some(status),
            }
        };
        let Some(status) = exited else {
            kill_tree(pid).await;
            let _ = child.kill().await;
            return Err(ProcessError::Cancelled);
        };
        let status = status.map_err(io)?;
        let drain = async {
            let o = match out_done {
                Some(r) => r,
                None => out.await,
            };
            let e = match err_done {
                Some(r) => r,
                None => err.await,
            };
            o.and(e)
        };
        match tokio::time::timeout(DRAIN, drain).await {
            Ok(result) => result.map_err(io)?,
            Err(_) => reporter
                .info("Stopped reading output: a process it started is still holding it open."),
        }
        status
    };
    Ok(ProcessOutput { code: status.code(), stdout: out_text, stderr: err_text })
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
        let mut all = String::new();
        collect(input, LogStream::Stderr, true, Some(&secret), &recorder, &mut all).await.unwrap();
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

    #[test]
    fn a_non_utf8_locale_gets_a_utf8_ctype() {
        assert_eq!(utf8_ctype(Some("en_US.UTF-8"), None), None);
        assert_eq!(utf8_ctype(None, Some("de_DE.utf8")), None);
        assert!(utf8_ctype(None, None).is_some());
        assert!(utf8_ctype(Some("C"), Some("en_US.UTF-8")).is_some(), "LC_CTYPE wins over LANG");
        assert!(utf8_ctype(Some(""), Some("POSIX")).is_some());
    }

    #[tokio::test]
    async fn shell_quotes_reach_the_shell_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let line = if cfg!(windows) { r#"echo "a b"> out.txt"# } else { r#"echo "a b" > out.txt"# };
        let out = run_shell(line, dir.path(), &Recorder::default(), &CancellationToken::new())
            .await
            .unwrap();
        assert!(out.success());
        let text = std::fs::read_to_string(dir.path().join("out.txt")).unwrap();
        let expected = if cfg!(windows) { "\"a b\"" } else { "a b" };
        assert_eq!(text.trim_end(), expected);
    }

    #[tokio::test]
    async fn cancel_kills_the_shell_and_its_children() {
        let dir = tempfile::tempdir().unwrap();
        // The grandchild would write the marker after two seconds unless killed.
        let line = if cfg!(windows) {
            "ping -n 3 127.0.0.1 >nul & echo done> marker.txt"
        } else {
            "sh -c 'sleep 2; echo done > marker.txt'"
        };
        let cancel = CancellationToken::new();
        let stopper = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            stopper.cancel();
        });
        let started = std::time::Instant::now();
        let err = run_shell(line, dir.path(), &Recorder::default(), &cancel).await.unwrap_err();
        assert!(matches!(err, ProcessError::Cancelled));
        assert!(started.elapsed() < Duration::from_secs(2));
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(!dir.path().join("marker.txt").exists(), "the grandchild was killed");
    }

    #[tokio::test]
    async fn a_background_child_holding_the_pipe_does_not_hang_the_step() {
        let dir = tempfile::tempdir().unwrap();
        let line = if cfg!(windows) {
            "start /b ping -n 8 127.0.0.1 & exit 0"
        } else {
            "sleep 8 & exit 0"
        };
        let started = std::time::Instant::now();
        let out = run_shell(line, dir.path(), &Recorder::default(), &CancellationToken::new())
            .await
            .unwrap();
        assert!(out.success());
        assert!(started.elapsed() < Duration::from_secs(6), "{:?}", started.elapsed());
    }
}
