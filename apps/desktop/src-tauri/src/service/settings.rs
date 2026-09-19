//! The Settings screen: tool paths and Doctor, privacy, diagnostics.

use std::path::Path;

use serde::Serialize;
use svnpush_core::run::ErrorView;
use svnpush_core::settings::{self, AppSettings};
use svnpush_core::tools::install::{InstallOutcome, InstallPlan};
use svnpush_core::tools::{self, ToolReport};
use svnpush_core::vault::Keychain;
use ts_rs::TS;

use crate::logging;
use crate::state::AppState;

/// Log lines included in the diagnostics bundle.
const DIAGNOSTIC_LOG_LINES: usize = 400;

/// What Doctor found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct DoctorReport {
    /// Subversion.
    pub svn: ToolReport,
    /// Git.
    pub git: ToolReport,
    /// Why the keychain is unusable, when it is.
    pub keychain_problem: Option<ErrorView>,
}

fn coded(e: &impl svnpush_core::Coded) -> ErrorView {
    ErrorView::from_coded(e)
}

/// The saved settings.
pub fn get(app: &AppState) -> Result<AppSettings, ErrorView> {
    settings::load(&app.paths).map_err(|e| coded(&e))
}

/// Saves settings. Empty tool paths mean "search".
pub fn save(app: &AppState, mut next: AppSettings) -> Result<AppSettings, ErrorView> {
    next.svn_path = next.svn_path.filter(|p| !p.trim().is_empty());
    next.git_path = next.git_path.filter(|p| !p.trim().is_empty());
    for (name, path) in [("svn", &next.svn_path), ("git", &next.git_path)] {
        if let Some(p) = path.as_deref().filter(|p| !Path::new(p).is_file()) {
            return Err(ErrorView::new(
                "SETTINGS_TOOL_NOT_FOUND",
                format!("No {name} executable at {p}."),
                Some(
                    "Choose the executable itself, or leave the field empty to search.".to_owned(),
                ),
            ));
        }
    }
    let current = get(app)?;
    next.privacy_notice_seen = current.privacy_notice_seen;
    settings::save(&app.paths, &next).map_err(|e| coded(&e))?;
    get(app)
}

/// Finds svn and git (with any configured paths) and checks the keychain.
pub async fn doctor(app: &AppState) -> Result<DoctorReport, ErrorView> {
    let current = get(app)?;
    Ok(DoctorReport {
        svn: tools::discover_svn(current.svn_path.as_deref().map(Path::new)).await,
        git: tools::discover_git(current.git_path.as_deref().map(Path::new)).await,
        keychain_problem: Keychain::status().err().map(|e| coded(&e)),
    })
}

/// How Subversion would be installed here, shown before the Install button runs.
pub fn svn_install_plan() -> InstallPlan {
    tools::install::install_plan()
}

/// Installs Subversion with the system package manager, when there is one.
pub async fn install_svn() -> InstallOutcome {
    tools::install::install_svn().await
}

/// A plain-text bundle for a bug report: version, platform, Doctor, settings
/// and the end of the newest log. Secrets are redacted.
pub async fn diagnostics(app: &AppState, version: &str) -> Result<String, ErrorView> {
    let report = doctor(app).await?;
    let current = get(app)?;
    let tool = |r: &ToolReport| format!("{:?}: {} (ok: {})", r.kind, r.message, r.ok);
    let mut out = vec![
        format!("SVNpush {version}"),
        format!("Platform: {} {}", std::env::consts::OS, std::env::consts::ARCH),
        String::new(),
        "Doctor".to_owned(),
        tool(&report.svn),
        tool(&report.git),
        format!(
            "Keychain: {}",
            report
                .keychain_problem
                .as_ref()
                .map_or_else(|| "available".to_owned(), |e| e.message.clone())
        ),
        String::new(),
        "Settings".to_owned(),
        format!("svn path: {}", current.svn_path.as_deref().unwrap_or("search")),
        format!("git path: {}", current.git_path.as_deref().unwrap_or("search")),
        format!("WordPress version lookup: {}", current.wordpress_version_lookup),
        String::new(),
        "Log".to_owned(),
    ];
    out.push(logging::tail(&app.paths.logs(), DIAGNOSTIC_LOG_LINES));
    Ok(svnpush_core::secret::redact_secrets(&out.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_rejects_missing_tools_and_diagnostics_redact() {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::test_support::app(dir.path());
        let bad =
            AppSettings { svn_path: Some("C:/nope/svn.exe".into()), ..AppSettings::default() };
        assert_eq!(save(&app, bad).unwrap_err().code, "SETTINGS_TOOL_NOT_FOUND");
        let off = AppSettings {
            wordpress_version_lookup: false,
            svn_path: Some("  ".into()),
            ..AppSettings::default()
        };
        let saved = save(&app, off).unwrap();
        assert!(!saved.wordpress_version_lookup);
        assert!(saved.svn_path.is_none());

        std::fs::create_dir_all(app.paths.logs()).unwrap();
        std::fs::write(app.paths.logs().join("svnpush.log"), "sent Bearer abcdefghijklmnop\n")
            .unwrap();
        let text = diagnostics(&app, "0.1.0").await.unwrap();
        assert!(text.starts_with("SVNpush 0.1.0"));
        assert!(text.contains("WordPress version lookup: false"));
        assert!(!text.contains("abcdefghijklmnop"));
    }
}
