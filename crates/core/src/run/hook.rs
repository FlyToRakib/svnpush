//! The project's pre-build command (plan §5.5 step 1), run through the
//! platform shell with its output streamed to the log.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use crate::report::Reporter;
use crate::tools::process::{self, ProcessError, ProcessSpec};

use super::RunFailure;

fn shell() -> (PathBuf, Vec<OsString>) {
    if cfg!(windows) {
        let comspec =
            std::env::var_os("ComSpec").map_or_else(|| PathBuf::from("cmd.exe"), PathBuf::from);
        (comspec, vec!["/D".into(), "/S".into(), "/C".into()])
    } else {
        (PathBuf::from("/bin/sh"), vec!["-c".into()])
    }
}

/// Runs `command` in `folder`. A non-zero exit stops the release.
pub async fn run_pre_build(
    command: &str,
    folder: &Path,
    reporter: &dyn Reporter,
    cancel: &CancellationToken,
) -> Result<(), RunFailure> {
    reporter.info(&format!("Running the pre-build command: {command}"));
    let (program, mut args) = shell();
    args.push(command.into());
    let spec =
        ProcessSpec { program: &program, args, cwd: Some(folder), stdin: None, log_output: true };
    let output = process::run(spec, reporter, cancel).await.map_err(|e| match e {
        ProcessError::Cancelled => RunFailure::cancelled(),
        other => RunFailure::new(
            "HOOK_NOT_RUNNABLE",
            format!("The pre-build command could not start: {other}"),
            Some("Check the command in project settings.".to_owned()),
        ),
    })?;
    if output.success() {
        Ok(())
    } else {
        let code = output.code.map_or_else(|| "a signal".to_owned(), |c| format!("code {c}"));
        Err(RunFailure::new(
            "HOOK_FAILED",
            format!("The pre-build command exited with {code}."),
            Some("Read the log, fix the build, and release again.".to_owned()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::NullReporter;

    #[tokio::test]
    async fn success_and_failure_exit_codes() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        run_pre_build("echo building", dir.path(), &NullReporter, &cancel).await.unwrap();
        let err = run_pre_build("exit 3", dir.path(), &NullReporter, &cancel).await.unwrap_err();
        assert_eq!(err.error.code, "HOOK_FAILED");
        assert!(err.error.message.contains("code 3"));
    }
}
