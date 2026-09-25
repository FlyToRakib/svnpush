//! Build package: the exact release package (pre-build command, packaging
//! rules, staged folder, zip and package checks) without releasing, so the
//! developer can inspect the files or test the zip on a WordPress site.

use std::path::Path;

use serde::Serialize;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::detect::{self, DetectOptions};
use crate::package::{self, Exclusions, Package};
use crate::project::{AppPaths, Project};
use crate::report::Reporter;
use crate::verify::{self, CheckResult};

use super::RunFailure;
use super::hook::run_pre_build;
use super::lock::ProjectLock;
use super::svn_steps::package_rows;

/// A package built outside a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct BuiltPackage {
    /// The version read from the plugin files, which names the zip.
    pub version: String,
    /// The staged folder and zip.
    pub package: Package,
    /// V10–V13 against it.
    pub checks: Vec<CheckResult>,
    /// Whether a blocking check failed, so a release would stop at Build.
    pub blocked: bool,
}

/// Builds the package for `project` at its current version. Holds the
/// project lock, so it never overlaps a release.
pub async fn build_package(
    project: &Project,
    paths: &AppPaths,
    reporter: &dyn Reporter,
    cancel: &CancellationToken,
) -> Result<BuiltPackage, RunFailure> {
    let _lock = ProjectLock::acquire(&paths.runs(&project.slug))?;
    let root = Path::new(&project.path);
    if let Some(command) =
        project.settings.pre_build_command.as_deref().filter(|c| !c.trim().is_empty())
    {
        run_pre_build(command, root, reporter, cancel).await?;
    }
    let facts = detect::detect(
        root,
        DetectOptions {
            svn_url: &project.svn_url,
            main_file: project.settings.main_file.as_deref(),
            version_locations: &project.settings.version_locations,
        },
    )?;
    let version = facts.header.version.clone().ok_or_else(|| {
        RunFailure::new(
            "BUILD_NO_VERSION",
            "The main plugin file has no Version header.",
            Some("Add \"Version: 1.0.0\" to the plugin header.".to_owned()),
        )
    })?;
    let package_root = project.package_root();
    if !package_root.is_dir() {
        return Err(RunFailure::new(
            "PACKAGE_ROOT_MISSING",
            format!("The package root {} does not exist.", package_root.display()),
            Some(
                "Check the package root and the pre-build command in project settings.".to_owned(),
            ),
        ));
    }
    let builds = paths.builds();
    let listing = package::list(
        &package_root,
        &Exclusions::load(&package_root, std::slice::from_ref(&builds))?,
    )?;
    let built = package::build(&listing, &builds, &project.slug, &version)?;
    let checks = package_rows(&facts, &version, &built, &project.settings)?;
    Ok(BuiltPackage { blocked: verify::is_blocked(&checks), version, package: built, checks })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectSettings;
    use crate::report::NullReporter;

    #[tokio::test]
    async fn builds_the_release_package_without_releasing() {
        let dir = tempfile::tempdir().unwrap();
        let plugin = dir.path().join("minimal");
        std::fs::create_dir_all(plugin.join(".agent")).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/plugins/minimal");
        for name in ["minimal.php", "readme.txt"] {
            std::fs::copy(fixture.join(name), plugin.join(name)).unwrap();
        }
        std::fs::write(plugin.join(".agent/notes.md"), "private").unwrap();
        let project = Project {
            path: plugin.display().to_string(),
            name: "Minimal".into(),
            svn_url: "https://plugins.svn.wordpress.org/minimal".into(),
            slug: "minimal".into(),
            settings: ProjectSettings::default(),
            last_release: None,
            created: String::new(),
        };
        let paths = AppPaths::new(&dir.path().join("appdata"));
        let built = build_package(&project, &paths, &NullReporter, &CancellationToken::new())
            .await
            .unwrap();
        let files: Vec<&str> = built.package.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(files, ["minimal.php", "readme.txt"]);
        assert!(built.package.zip_path.contains(&built.version), "the zip is named by the version");
        assert!(!built.blocked, "{:?}", built.checks);
        assert!(Path::new(&built.package.zip_path).is_file());
        assert!(!plugin.join("dist").exists(), "nothing is written into the plugin folder");
    }
}
