//! The SVNpush desktop shell: window, commands and events over `svnpush-core`.

mod commands;
mod events;
mod service;
mod state;
#[cfg(test)]
mod test_support;

use std::sync::Arc;

use svnpush_core::project::AppPaths;
use svnpush_core::vault::Keychain;
use tauri::Manager;

use crate::state::AppState;

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
            commands::confirm_publish,
            commands::cancel_run,
            commands::resume_run,
            commands::discard_run,
            commands::reset_working_copy,
        ]
    };
}

/// Builds and runs the Tauri application.
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data = app.path().data_dir()?;
            let state = Arc::new(AppState::new(AppPaths::new(&data), Arc::new(Keychain)));
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
