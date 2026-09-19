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
use tauri::Manager;

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
        ]
    };
}

/// Builds and runs the Tauri application.
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let paths = AppPaths::new(&app.path().data_dir()?);
            app.manage(LogGuard(logging::init(&paths.logs())));
            tracing::info!(version = %app.package_info().version, "SVNpush started");
            let ai = AiClient::new()?;
            let state = Arc::new(AppState::new(paths, Arc::new(Keychain), ai));
            let pruning = state.clone();
            tauri::async_runtime::spawn(async move { service::runs::prune(&pruning).await });
            app.manage(state);
            Ok(())
        })
        .invoke_handler(handlers!())
        .run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("SVNpush failed to start: {err}");
        std::process::exit(1);
    }
}
