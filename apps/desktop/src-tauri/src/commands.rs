//! Tauri commands: thin wrappers that pass arguments to the services and
//! return their DTOs or a `{ code, message, fix }` error.

use std::sync::Arc;

use serde::Serialize;
use svnpush_core::ai::provider::{Fleet, ModelList};
use svnpush_core::ai::records::ProvidersFile;
use svnpush_core::project::ProjectSettings;
use svnpush_core::run::{Decision, ErrorView, ReleaseDraft, RunJournal, RunState};
use svnpush_core::settings::AppSettings;
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;
use ts_rs::TS;

use crate::events::{EventSink, TauriSink};
use crate::service::projects::{self, FolderInspection, ProjectSummary};
use crate::service::providers::{self, AdapterInfo, ProviderInput, ProviderTarget};
use crate::service::runs;
use crate::service::settings::{self, DoctorReport};
use crate::service::vault::{self, VaultView};
use crate::state::AppState;

type Shared<'a> = State<'a, Arc<AppState>>;

fn sink(app: AppHandle) -> Arc<dyn EventSink> {
    Arc::new(TauriSink(app))
}

/// The app version and whether a newer signed release exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct UpdateInfo {
    /// The running version.
    pub current: String,
    /// The newer version, when one is published.
    pub available: Option<String>,
}

fn update_error(e: impl std::fmt::Display) -> ErrorView {
    ErrorView::new(
        "UPDATE_CHECK_FAILED",
        format!("Could not check for updates: {e}"),
        Some("Check your connection and try again.".to_owned()),
    )
}

#[tauri::command]
pub async fn vault_view(state: Shared<'_>) -> Result<VaultView, ErrorView> {
    vault::view(&state)
}

#[tauri::command]
pub async fn save_account(
    state: Shared<'_>,
    host: String,
    username: String,
    password: String,
) -> Result<VaultView, ErrorView> {
    vault::save(&state, &host, &username, &password)
}

#[tauri::command]
pub async fn remove_account(
    state: Shared<'_>,
    host: String,
    username: String,
) -> Result<VaultView, ErrorView> {
    vault::remove(&state, &host, &username)
}

#[tauri::command]
pub async fn test_account(
    state: Shared<'_>,
    host: String,
    username: String,
) -> Result<String, ErrorView> {
    vault::test(&state, &host, &username).await
}

#[tauri::command]
pub async fn provider_adapters() -> Result<Vec<AdapterInfo>, ErrorView> {
    Ok(providers::adapters())
}

#[tauri::command]
pub async fn list_providers(state: Shared<'_>) -> Result<ProvidersFile, ErrorView> {
    providers::list(&state)
}

#[tauri::command]
pub async fn save_provider(
    state: Shared<'_>,
    input: ProviderInput,
) -> Result<ProvidersFile, ErrorView> {
    providers::save(&state, input)
}

#[tauri::command]
pub async fn remove_provider(state: Shared<'_>, id: String) -> Result<ProvidersFile, ErrorView> {
    providers::remove(&state, &id)
}

#[tauri::command]
pub async fn set_default_provider(
    state: Shared<'_>,
    id: String,
) -> Result<ProvidersFile, ErrorView> {
    providers::set_default(&state, &id)
}

#[tauri::command]
pub async fn clear_provider_attention(
    state: Shared<'_>,
    id: String,
) -> Result<ProvidersFile, ErrorView> {
    providers::clear_attention(&state, &id)
}

#[tauri::command]
pub async fn save_provider_fallback(
    state: Shared<'_>,
    order: Vec<String>,
) -> Result<ProvidersFile, ErrorView> {
    providers::save_fallback(&state, order)
}

#[tauri::command]
pub async fn test_provider(state: Shared<'_>, id: String) -> Result<String, ErrorView> {
    providers::test(&state, &id).await
}

#[tauri::command]
pub async fn list_provider_models(
    state: Shared<'_>,
    target: ProviderTarget,
) -> Result<ModelList, ErrorView> {
    providers::list_models(&state, target).await
}

#[tauri::command]
pub async fn provider_fleet(state: Shared<'_>, target: ProviderTarget) -> Result<Fleet, ErrorView> {
    providers::fleet(&state, target).await
}

#[tauri::command]
pub async fn get_settings(state: Shared<'_>) -> Result<AppSettings, ErrorView> {
    settings::get(&state)
}

#[tauri::command]
pub async fn save_settings(
    state: Shared<'_>,
    settings: AppSettings,
) -> Result<AppSettings, ErrorView> {
    settings::save(&state, settings)
}

#[tauri::command]
pub async fn run_doctor(state: Shared<'_>) -> Result<DoctorReport, ErrorView> {
    settings::doctor(&state).await
}

#[tauri::command]
pub async fn diagnostics(app: AppHandle, state: Shared<'_>) -> Result<String, ErrorView> {
    settings::diagnostics(&state, &app.package_info().version.to_string()).await
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<UpdateInfo, ErrorView> {
    let current = app.package_info().version.to_string();
    let update = app.updater().map_err(update_error)?.check().await.map_err(update_error)?;
    Ok(UpdateInfo { current, available: update.map(|u| u.version) })
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), ErrorView> {
    let Some(update) = app.updater().map_err(update_error)?.check().await.map_err(update_error)?
    else {
        return Ok(());
    };
    update.download_and_install(|_, _| {}, || {}).await.map_err(update_error)?;
    app.restart();
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
    assets_only: bool,
) -> Result<RunState, ErrorView> {
    runs::start(state.inner().clone(), sink(app), &path, dry_run, assets_only).await
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
pub async fn ai_decision(
    state: Shared<'_>,
    path: String,
    decision: Decision,
) -> Result<(), ErrorView> {
    runs::ai_decision(&state, &path, decision).await
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
