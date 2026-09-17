//! The SVNpush desktop shell: window, commands and events over `svnpush-core`.

/// Builds and runs the Tauri application.
pub fn run() {
    if let Err(err) = tauri::Builder::default().run(tauri::generate_context!()) {
        eprintln!("SVNpush failed to start: {err}");
        std::process::exit(1);
    }
}
