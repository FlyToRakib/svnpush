//! Project tools outside a release: check the readme against the
//! WordPress.org validator's rules, and build the release package.

use std::path::Path;

use svnpush_core::edit;
use svnpush_core::readme::{self, README_FILE, ReadmeReport};
use svnpush_core::report::NullReporter;
use svnpush_core::run::ErrorView;
use svnpush_core::run::build_only::{self, BuiltPackage};
use svnpush_core::settings;
use svnpush_core::wporg_assets::{self, AssetReport};
use tokio_util::sync::CancellationToken;

use crate::service::projects;
use crate::state::AppState;

/// Validates the project's `readme.txt` as WordPress.org would.
pub async fn check_readme(app: &AppState, path: &str) -> Result<ReadmeReport, ErrorView> {
    let project = projects::load(app, path).await?;
    let text = edit::read_text(Path::new(&project.path), README_FILE).map_err(|_| {
        ErrorView::new(
            "README_MISSING",
            "This plugin has no readme.txt.",
            Some("Add a readme.txt that follows the WordPress.org readme standard.".to_owned()),
        )
    })?;
    let config = settings::load(&app.paths).map_err(|e| ErrorView::from_coded(&e))?;
    let current = settings::current_wordpress_version(&app.paths, &config).await;
    Ok(readme::validate(&text, current.as_deref()))
}

/// Builds the release package without releasing. Refused while a release runs.
pub async fn build_package(app: &AppState, path: &str) -> Result<BuiltPackage, ErrorView> {
    if app.runs().get(path).is_some_and(|slot| slot.control.is_some()) {
        return Err(ErrorView::new(
            "RUN_ALREADY_ACTIVE",
            "A release is running for this plugin.",
            Some("Wait for it to finish, then build the package.".to_owned()),
        ));
    }
    let project = projects::load(app, path).await?;
    build_only::build_package(&project, &app.paths, &NullReporter, &CancellationToken::new())
        .await
        .map_err(|f| f.error)
}

/// The number of `N. caption` lines in the readme's Screenshots section.
fn screenshot_captions(project_path: &Path) -> Option<usize> {
    let text = edit::read_text(project_path, README_FILE).ok()?;
    let parsed = readme::parse(&text);
    let section = parsed.sections.iter().find(|s| s.title.eq_ignore_ascii_case("Screenshots"))?;
    Some(
        section
            .body
            .lines()
            .filter(|l| l.trim().split_once('.').is_some_and(|(n, _)| n.parse::<u32>().is_ok()))
            .count(),
    )
}

/// Checks the WordPress.org images in the project's assets folder.
pub async fn check_assets(app: &AppState, path: &str) -> Result<AssetReport, ErrorView> {
    let project = projects::load(app, path).await?;
    let folder = project.assets_folder_target();
    Ok(wporg_assets::inspect(folder.as_deref(), screenshot_captions(Path::new(&project.path))))
}

/// Creates the assets folder (`.wordpress-org` unless another is set) and checks it.
pub async fn create_assets_folder(app: &AppState, path: &str) -> Result<AssetReport, ErrorView> {
    let project = projects::load(app, path).await?;
    let Some(folder) = project.assets_folder_target() else {
        return Err(ErrorView::new(
            "ASSETS_TURNED_OFF",
            "This project does not sync an assets folder.",
            Some("Turn it back on in Project settings → Assets folder.".to_owned()),
        ));
    };
    std::fs::create_dir_all(&folder).map_err(|e| {
        ErrorView::new(
            "ASSETS_CREATE_FAILED",
            format!("Could not create {}: {e}", folder.display()),
            None,
        )
    })?;
    check_assets(app, path).await
}
