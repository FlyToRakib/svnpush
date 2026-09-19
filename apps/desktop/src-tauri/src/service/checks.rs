//! Project tools outside a release: check the readme against the
//! WordPress.org validator's rules, and build the release package.

use std::path::Path;

use svnpush_core::edit;
use svnpush_core::readme::{self, README_FILE, ReadmeReport};
use svnpush_core::report::NullReporter;
use svnpush_core::run::ErrorView;
use svnpush_core::run::build_only::{self, BuiltPackage};
use svnpush_core::settings;
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
