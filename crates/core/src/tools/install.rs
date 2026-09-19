//! Installing Subversion for the developer, where the system has a package
//! manager SVNpush can drive: winget on Windows, Homebrew on macOS. Linux
//! packages depend on `subversion`, and otherwise the developer runs the one
//! command shown (SVNpush never asks for an administrator password itself).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::process::{self, ProcessSpec};
use crate::report::NullReporter;

/// The winget package: the Apache Subversion command-line tools (the `SlikSvn`
/// build, Apache-2.0), installed to `C:\Program Files\SlikSvn\bin`.
pub const WINGET_PACKAGE: &str = "Slik.Subversion";

/// How Subversion can be installed on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum InstallMethod {
    /// Windows Package Manager.
    Winget,
    /// Homebrew on macOS.
    Homebrew,
    /// The developer runs the command shown.
    Manual,
}

/// What the Install button will do, or what the developer should run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct InstallPlan {
    /// How.
    pub method: InstallMethod,
    /// The command, shown to the developer before anything runs.
    pub command: String,
}

/// The result of an install attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct InstallOutcome {
    /// Whether the package manager reported success.
    pub ok: bool,
    /// The last lines of its output, for the developer.
    pub detail: String,
}

fn winget() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join(r"Microsoft\WindowsApps\winget.exe"));
    super::find_on_path("winget").or_else(|| local.filter(|p| p.is_file()))
}

fn homebrew() -> Option<PathBuf> {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

fn winget_args() -> Vec<&'static str> {
    vec![
        "install",
        "--id",
        WINGET_PACKAGE,
        "--exact",
        "--source",
        "winget",
        "--accept-package-agreements",
        "--accept-source-agreements",
    ]
}

fn runner() -> Option<(PathBuf, Vec<&'static str>)> {
    if cfg!(windows) {
        winget().map(|p| (p, winget_args()))
    } else if cfg!(target_os = "macos") {
        homebrew().map(|p| (p, vec!["install", "subversion"]))
    } else {
        None
    }
}

/// How Subversion would be installed here.
pub fn install_plan() -> InstallPlan {
    match runner() {
        Some((_, args)) if cfg!(windows) => InstallPlan {
            method: InstallMethod::Winget,
            command: format!("winget {}", args.join(" ")),
        },
        Some(_) => InstallPlan {
            method: InstallMethod::Homebrew,
            command: "brew install subversion".to_owned(),
        },
        None if cfg!(windows) => InstallPlan {
            method: InstallMethod::Manual,
            command: format!("winget install --id {WINGET_PACKAGE} --exact"),
        },
        None if cfg!(target_os = "macos") => InstallPlan {
            method: InstallMethod::Manual,
            command: "brew install subversion".to_owned(),
        },
        None => InstallPlan {
            method: InstallMethod::Manual,
            command: "sudo apt install subversion".to_owned(),
        },
    }
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Runs the package manager. Windows may show its own permission prompt.
pub async fn install_svn() -> InstallOutcome {
    let Some((program, args)) = runner() else {
        return InstallOutcome {
            ok: false,
            detail: format!("Run this command yourself: {}", install_plan().command),
        };
    };
    let spec = ProcessSpec {
        program: Path::new(&program),
        args: args.iter().map(OsString::from).collect(),
        cwd: None,
        stdin: None,
        log_output: false,
    };
    match process::run(spec, &NullReporter, &CancellationToken::new()).await {
        Ok(out) => InstallOutcome {
            ok: out.success(),
            detail: tail(&format!("{}\n{}", out.stdout, out.stderr), 6),
        },
        Err(e) => InstallOutcome { ok: false, detail: e.to_string() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_always_names_a_command() {
        let plan = install_plan();
        assert!(!plan.command.is_empty());
        if cfg!(windows) {
            assert!(plan.command.contains(WINGET_PACKAGE));
        }
    }

    #[test]
    fn winget_installs_the_exact_package_from_the_official_source() {
        let args = winget_args().join(" ");
        assert!(args.contains("--id Slik.Subversion --exact --source winget"));
    }

    #[test]
    fn tail_keeps_the_last_non_empty_lines() {
        assert_eq!(tail("a\n\nb\nc\n  \nd\n", 2), "c\nd");
        assert_eq!(tail("", 3), "");
    }
}
