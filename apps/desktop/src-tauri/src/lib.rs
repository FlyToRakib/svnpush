//! The SVNpush desktop shell: window, commands and events over `svnpush-core`.

mod commands;
mod events;
mod logging;
mod service;
mod state;
#[cfg(test)]
mod test_support;

use std::sync::Arc;

use svnpush_core::ai::client::AiClient;
use svnpush_core::project::AppPaths;
use svnpush_core::vault::Keychain;
use tauri::{Emitter, Manager, WindowEvent};

use crate::state::AppState;

/// Keeps the log writer alive for the app's lifetime.
struct LogGuard(#[allow(dead_code)] Option<tracing_appender::non_blocking::WorkerGuard>);

/// Every command the UI may call. `build.rs` lists the same names so each
/// gets a permission, and `capabilities/default.json` grants exactly these.
macro_rules! handlers {
    () => {
        tauri::generate_handler![
            commands::list_projects,
            commands::inspect_folder,
            commands::add_project,
            commands::update_project,
            commands::remove_project,
            commands::project_history,
            commands::start_run,
            commands::current_run,
            commands::approve_draft,
            commands::ai_decision,
            commands::check_readme,
            commands::build_package,
            commands::check_assets,
            commands::create_assets_folder,
            commands::preview_release_files,
            commands::confirm_release_files,
            commands::confirm_publish,
            commands::cancel_run,
            commands::resume_run,
            commands::discard_run,
            commands::reset_working_copy,
            commands::vault_view,
            commands::save_account,
            commands::remove_account,
            commands::test_account,
            commands::provider_adapters,
            commands::list_providers,
            commands::save_provider,
            commands::remove_provider,
            commands::set_default_provider,
            commands::clear_provider_attention,
            commands::save_provider_fallback,
            commands::test_provider,
            commands::list_provider_models,
            commands::provider_fleet,
            commands::get_settings,
            commands::save_settings,
            commands::run_doctor,
            commands::svn_install_plan,
            commands::install_svn,
            commands::diagnostics,
            commands::check_update,
            commands::install_update,
            commands::force_close,
        ]
    };
}

/// Closing the window while a release runs would kill a commit halfway, so
/// the close is held and the UI asks first (it calls `force_close` on yes).
fn guard_close(window: &tauri::Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event
        && window.try_state::<Arc<AppState>>().is_some_and(|state| state.any_active())
    {
        api.prevent_close();
        let _ = window.emit(events::CLOSE_BLOCKED_EVENT, ());
    }
}

/// A failure before the window opens is otherwise invisible in a release
/// build, which has no console.
fn report_startup_failure(err: &dyn std::fmt::Display) {
    tracing::error!("SVNpush failed to start: {err}");
    eprintln!("SVNpush failed to start: {err}");
    #[cfg(desktop)]
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("SVNpush could not start")
        .set_description(format!("SVNpush failed to start: {err}"))
        .show();
}

/// Opens the logs and creates the app state.
fn setup(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let paths = AppPaths::with_local(&app.path().data_dir()?, &app.path().local_data_dir()?);
    app.manage(LogGuard(logging::init(&paths.logs())));
    tracing::info!(version = %app.package_info().version, "SVNpush started");
    let ai = AiClient::new()?;
    let state = Arc::new(AppState::new(paths, Arc::new(Keychain), ai));
    let pruning = state.clone();
    tauri::async_runtime::spawn(async move { service::runs::prune(&pruning).await });
    app.manage(state);
    Ok(())
}

/// Builds and runs the Tauri application.
pub fn run() {
    let builder = tauri::Builder::default();
    // First, so a second launch only focuses this window: two processes
    // would overwrite each other's projects, accounts and providers.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }));
    let result = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // Tauri panics when setup returns an error, which a release build
            // shows nowhere, so the failure is reported here instead.
            if let Err(err) = setup(app) {
                report_startup_failure(&err);
                std::process::exit(1);
            }
            Ok(())
        })
        .on_window_event(guard_close)
        .invoke_handler(handlers!())
        .run(tauri::generate_context!());
    if let Err(err) = result {
        report_startup_failure(&err);
        std::process::exit(1);
    }
}
