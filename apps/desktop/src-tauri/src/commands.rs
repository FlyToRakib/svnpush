//! Tauri commands: thin wrappers that pass arguments to the services and
//! return their DTOs or a `{ code, message, fix }` error.

use std::sync::Arc;

use svnpush_core::project::ProjectSettings;
use svnpush_core::run::{ErrorView, ReleaseDraft, RunJournal, RunState};
use tauri::{AppHandle, State};

use crate::events::{EventSink, TauriSink};
use crate::service::projects::{self, FolderInspection, ProjectSummary};
use crate::service::runs;
use crate::state::AppState;

type Shared<'a> = State<'a, Arc<AppState>>;

fn sink(app: AppHandle) -> Arc<dyn EventSink> {
    Arc::new(TauriSink(app))
}

#[tauri::command]
pub async fn list_projects(state: Shared<'_>) -> Result<Vec<ProjectSummary>, ErrorView> {
    projects::list(&state).await
}

#[tauri::command]
pub async fn inspect_folder(folder: String) -> Result<FolderInspection, ErrorView> {
    projects::inspect_folder(&folder)
}

#[tauri::command]
pub async fn add_project(
    state: Shared<'_>,
    folder: String,
    svn_url: String,
    main_file: Option<String>,
) -> Result<ProjectSummary, ErrorView> {
    projects::add(&state, &folder, &svn_url, main_file).await
}

#[tauri::command]
pub async fn update_project(
    state: Shared<'_>,
    path: String,
    svn_url: String,
    settings: ProjectSettings,
) -> Result<ProjectSummary, ErrorView> {
    projects::update(&state, &path, &svn_url, settings).await
}

#[tauri::command]
pub async fn remove_project(state: Shared<'_>, path: String) -> Result<(), ErrorView> {
    projects::remove(&state, &path).await
}

#[tauri::command]
pub async fn project_history(
    state: Shared<'_>,
    path: String,
) -> Result<Vec<RunJournal>, ErrorView> {
    projects::history(&state, &path).await
}

#[tauri::command]
pub async fn start_run(
    app: AppHandle,
    state: Shared<'_>,
    path: String,
    dry_run: bool,
) -> Result<RunState, ErrorView> {
    runs::start(state.inner().clone(), sink(app), &path, dry_run).await
}

#[tauri::command]
pub async fn current_run(state: Shared<'_>, path: String) -> Result<Option<RunState>, ErrorView> {
    Ok(runs::current(&state, &path))
}

#[tauri::command]
pub async fn cancel_run(state: Shared<'_>, path: String) -> Result<(), ErrorView> {
    runs::cancel(&state, &path)
}

#[tauri::command]
pub async fn approve_draft(
    state: Shared<'_>,
    path: String,
    draft: ReleaseDraft,
) -> Result<(), ErrorView> {
    runs::approve(&state, &path, draft).await
}

#[tauri::command]
pub async fn confirm_publish(
    state: Shared<'_>,
    path: String,
    trunk_message: String,
    tag_message: String,
) -> Result<(), ErrorView> {
    runs::publish(&state, &path, trunk_message, tag_message).await
}

#[tauri::command]
pub async fn resume_run(
    app: AppHandle,
    state: Shared<'_>,
    path: String,
    run_id: String,
) -> Result<RunState, ErrorView> {
    runs::resume(state.inner().clone(), sink(app), &path, &run_id).await
}

#[tauri::command]
pub async fn discard_run(
    app: AppHandle,
    state: Shared<'_>,
    path: String,
    run_id: String,
) -> Result<(), ErrorView> {
    runs::discard(state.inner(), &sink(app), &path, &run_id).await
}

#[tauri::command]
pub async fn reset_working_copy(state: Shared<'_>, path: String) -> Result<(), ErrorView> {
    runs::reset_working_copy(&state, &path).await
}
