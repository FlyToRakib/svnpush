//! Projects: add, inspect, update, remove, and their release history.

use std::path::{Path, PathBuf};

use serde::Serialize;
use svnpush_core::clock;
use svnpush_core::detect::{self, DetectOptions};
use svnpush_core::project::{self, Project, ProjectSettings};
use svnpush_core::run::{ErrorView, ProjectLock, RunJournal, journal};
use ts_rs::TS;

use crate::state::AppState;

/// A project row in the Projects list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ProjectSummary {
    /// The saved project.
    pub project: Project,
    /// The version in the plugin header right now, when it can be read.
    pub version: Option<String>,
    /// Why detection failed, when it did.
    pub problem: Option<ErrorView>,
    /// The newest journal, when it has no outcome or still needs its tag.
    pub unfinished: Option<RunJournal>,
    /// Whether another window is releasing this plugin.
    pub locked: bool,
}

/// What a folder looks like before it is added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FolderInspection {
    /// The folder.
    pub folder: String,
    /// The SVN URL offered from the folder name.
    pub suggested_svn_url: Option<String>,
    /// PHP files with a plugin header (more than one needs a choice).
    pub main_file_candidates: Vec<String>,
    /// Plugin name, when a single main file was found.
    pub name: Option<String>,
    /// Header version, when a single main file was found.
    pub version: Option<String>,
}

fn not_found(path: &str) -> ErrorView {
    ErrorView::new(
        "PROJECT_NOT_FOUND",
        format!("No project at {path}."),
        Some("Add the project again.".to_owned()),
    )
}

fn slug_or_error(svn_url: &str) -> Result<String, ErrorView> {
    detect::slug_from_svn_url(svn_url).ok_or_else(|| {
        ErrorView::new(
            "PROJECT_INVALID_SVN_URL",
            format!("\"{svn_url}\" does not end in a plugin slug."),
            Some("Use the plugin's SVN URL, for example https://plugins.svn.wordpress.org/my-plugin.".to_owned()),
        )
    })
}

/// Reads a folder: suggested URL and main file candidates.
pub fn inspect_folder(folder: &str) -> Result<FolderInspection, ErrorView> {
    let root = PathBuf::from(folder);
    if !root.is_dir() {
        return Err(ErrorView::new(
            "DETECT_NOT_A_FOLDER",
            format!("{folder} is not a folder."),
            None,
        ));
    }
    let candidates = detect::main_file_candidates(&root).map_err(|e| ErrorView::from_coded(&e))?;
    let folder_name =
        root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let header = match candidates.as_slice() {
        [single] => {
            std::fs::read_to_string(root.join(single)).ok().map(|t| detect::header::parse(&t))
        }
        _ => None,
    };
    Ok(FolderInspection {
        folder: folder.to_owned(),
        suggested_svn_url: detect::suggested_svn_url(&folder_name),
        main_file_candidates: candidates,
        name: header.as_ref().and_then(|h| h.name.clone()),
        version: header.and_then(|h| h.version),
    })
}

fn summarise(app: &AppState, project: Project) -> ProjectSummary {
    let effective = project::with_team_config(&project);
    let detected = effective.as_ref().map_err(|e| ErrorView::from_coded(e)).and_then(|p| {
        detect::detect(
            Path::new(&p.path),
            DetectOptions {
                svn_url: &p.svn_url,
                main_file: p.settings.main_file.as_deref(),
                version_locations: &p.settings.version_locations,
            },
        )
        .map_err(|e| ErrorView::from_coded(&e))
    });
    let unfinished = journal::list(&app.paths, &project.slug)
        .ok()
        .and_then(|all| all.into_iter().next())
        .filter(|j| !j.discarded && (j.is_interrupted() || j.needs_tag()));
    ProjectSummary {
        locked: ProjectLock::is_held(&app.paths.runs(&project.slug)),
        version: detected.as_ref().ok().and_then(|f| f.header.version.clone()),
        problem: detected.err(),
        unfinished,
        project,
    }
}

/// Every project with its current version and unfinished runs.
pub async fn list(app: &AppState) -> Result<Vec<ProjectSummary>, ErrorView> {
    let _guard = app.projects_lock.lock().await;
    let projects = project::load_projects(&app.paths).map_err(|e| ErrorView::from_coded(&e))?;
    Ok(projects.into_iter().map(|p| summarise(app, p)).collect())
}

/// Adds a project after checking the folder detects with this SVN URL.
pub async fn add(
    app: &AppState,
    folder: &str,
    svn_url: &str,
    main_file: Option<String>,
) -> Result<ProjectSummary, ErrorView> {
    let svn_url = svn_url.trim();
    let slug = slug_or_error(svn_url)?;
    let settings = ProjectSettings { main_file, ..ProjectSettings::default() };
    let facts = detect::detect(
        Path::new(folder),
        DetectOptions { svn_url, main_file: settings.main_file.as_deref(), version_locations: &[] },
    )
    .map_err(|e| ErrorView::from_coded(&e))?;

    let _guard = app.projects_lock.lock().await;
    let mut projects = project::load_projects(&app.paths).map_err(|e| ErrorView::from_coded(&e))?;
    if projects.iter().any(|p| p.path == folder) {
        return Err(ErrorView::new(
            "PROJECT_EXISTS",
            format!("{} is already a project.", facts.name),
            Some("Open it from the Projects list.".to_owned()),
        ));
    }
    let project = Project {
        path: folder.to_owned(),
        name: facts.name,
        svn_url: svn_url.to_owned(),
        slug,
        settings,
        last_release: None,
        created: clock::iso8601(clock::now()),
    };
    projects.push(project.clone());
    project::save_projects(&app.paths, &projects).map_err(|e| ErrorView::from_coded(&e))?;
    Ok(summarise(app, project))
}

/// Saves a project's SVN URL and settings.
pub async fn update(
    app: &AppState,
    path: &str,
    svn_url: &str,
    settings: ProjectSettings,
) -> Result<ProjectSummary, ErrorView> {
    let slug = slug_or_error(svn_url.trim())?;
    for location in &settings.version_locations {
        svnpush_core::version::validate_location(location)
            .map_err(|e| ErrorView::from_coded(&e))?;
    }
    let _guard = app.projects_lock.lock().await;
    let mut projects = project::load_projects(&app.paths).map_err(|e| ErrorView::from_coded(&e))?;
    let project = projects.iter_mut().find(|p| p.path == path).ok_or_else(|| not_found(path))?;
    svn_url.trim().clone_into(&mut project.svn_url);
    project.slug = slug;
    project.settings = settings;
    let updated = project.clone();
    project::save_projects(&app.paths, &projects).map_err(|e| ErrorView::from_coded(&e))?;
    Ok(summarise(app, updated))
}

/// Removes a project from the list. Files, journals and the working copy stay on disk.
pub async fn remove(app: &AppState, path: &str) -> Result<(), ErrorView> {
    let _guard = app.projects_lock.lock().await;
    let mut projects = project::load_projects(&app.paths).map_err(|e| ErrorView::from_coded(&e))?;
    let before = projects.len();
    projects.retain(|p| p.path != path);
    if projects.len() == before {
        return Err(not_found(path));
    }
    project::save_projects(&app.paths, &projects).map_err(|e| ErrorView::from_coded(&e))
}

/// A saved project with `.svnpush.json` applied.
pub async fn load(app: &AppState, path: &str) -> Result<Project, ErrorView> {
    let _guard = app.projects_lock.lock().await;
    let projects = project::load_projects(&app.paths).map_err(|e| ErrorView::from_coded(&e))?;
    let found = projects.into_iter().find(|p| p.path == path).ok_or_else(|| not_found(path))?;
    project::with_team_config(&found).map_err(|e| ErrorView::from_coded(&e))
}

/// Past runs, newest first.
pub async fn history(app: &AppState, path: &str) -> Result<Vec<RunJournal>, ErrorView> {
    let project = load(app, path).await?;
    journal::list(&app.paths, &project.slug).map_err(|e| ErrorView::from_coded(&e))
}

/// Records how a run ended on the project, for the Projects list.
pub async fn record_outcome(
    app: &AppState,
    path: &str,
    finished: &RunJournal,
) -> Result<(), ErrorView> {
    let Some(outcome) = &finished.outcome else { return Ok(()) };
    let _guard = app.projects_lock.lock().await;
    let mut projects = project::load_projects(&app.paths).map_err(|e| ErrorView::from_coded(&e))?;
    if let Some(project) = projects.iter_mut().find(|p| p.path == path) {
        project.last_release = Some(project::LastRelease {
            version: finished.version.clone().unwrap_or_default(),
            finished: finished.finished.clone().unwrap_or_else(|| clock::iso8601(clock::now())),
            outcome: outcome.label().to_owned(),
        });
    }
    project::save_projects(&app.paths, &projects).map_err(|e| ErrorView::from_coded(&e))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::Value;

    use super::*;
    use crate::test_support::MemoryVault;

    fn fixture_copy(dir: &Path) -> String {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/plugins/minimal");
        let dest = dir.join("minimal");
        std::fs::create_dir_all(&dest).unwrap();
        for name in ["minimal.php", "readme.txt"] {
            std::fs::copy(src.join(name), dest.join(name)).unwrap();
        }
        dest.display().to_string()
    }

    fn app(dir: &Path) -> AppState {
        AppState::new(project::AppPaths::new(&dir.join("data")), Arc::new(MemoryVault::default()))
    }

    #[tokio::test]
    async fn add_list_update_remove_with_the_dto_shape_the_ui_reads() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let folder = fixture_copy(dir.path());

        let inspected = inspect_folder(&folder).unwrap();
        assert_eq!(
            inspected.suggested_svn_url.as_deref(),
            Some("https://plugins.svn.wordpress.org/minimal")
        );
        let json = serde_json::to_value(&inspected).unwrap();
        for key in ["folder", "suggested_svn_url", "main_file_candidates", "name", "version"] {
            assert!(json.get(key).is_some(), "{key}");
        }

        let added =
            add(&app, &folder, "https://plugins.svn.wordpress.org/minimal", None).await.unwrap();
        let json = serde_json::to_value(&added).unwrap();
        assert_eq!(json["project"]["slug"], "minimal");
        assert_eq!(json["version"], "1.0.0");
        assert_eq!(json["problem"], Value::Null);
        assert_eq!(json["unfinished"], Value::Null);
        assert_eq!(json["locked"], false);
        assert_eq!(json["project"]["settings"]["post_publish_open_page"], true);

        let again = add(&app, &folder, "https://plugins.svn.wordpress.org/minimal", None)
            .await
            .unwrap_err();
        assert_eq!(again.code, "PROJECT_EXISTS");

        let mut settings = added.project.settings.clone();
        settings.required_paths = vec!["inc".into()];
        let updated = update(&app, &folder, "https://plugins.svn.wordpress.org/renamed", settings)
            .await
            .unwrap();
        assert_eq!(updated.project.slug, "renamed");
        assert_eq!(list(&app).await.unwrap()[0].project.settings.required_paths, ["inc"]);

        let bad = update(&app, &folder, "not a url", ProjectSettings::default()).await.unwrap_err();
        assert_eq!(bad.code, "PROJECT_INVALID_SVN_URL");

        remove(&app, &folder).await.unwrap();
        assert!(list(&app).await.unwrap().is_empty());
        assert_eq!(remove(&app, &folder).await.unwrap_err().code, "PROJECT_NOT_FOUND");
    }

    #[tokio::test]
    async fn a_folder_without_a_plugin_is_refused_with_a_fix() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let empty = dir.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let err = add(
            &app,
            &empty.display().to_string(),
            "https://plugins.svn.wordpress.org/empty",
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "DETECT_NO_MAIN_FILE");
        assert!(err.fix.is_some());
        let json = serde_json::to_value(&err).unwrap();
        let mut keys: Vec<&String> = json.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["code", "fix", "message"]);
    }

    #[tokio::test]
    async fn record_outcome_sets_last_release() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let folder = fixture_copy(dir.path());
        add(&app, &folder, "https://plugins.svn.wordpress.org/minimal", None).await.unwrap();
        let mut journal = RunJournal::start("20260917-101530", "minimal", &folder, true);
        journal.version = Some("1.0.1".into());
        journal.outcome = Some(journal::Outcome::DryRun);
        journal.finished = Some("2026-09-17T10:16:00Z".into());
        record_outcome(&app, &folder, &journal).await.unwrap();
        let last = list(&app).await.unwrap()[0].project.last_release.clone().unwrap();
        assert_eq!((last.version.as_str(), last.outcome.as_str()), ("1.0.1", "DryRun"));
    }
}
