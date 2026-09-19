fn main() {
    // Every app command gets an `allow-<command>` permission; the default
    // capability grants exactly this list and nothing else.
    let manifest = tauri_build::AppManifest::new().commands(&[
        "list_projects",
        "inspect_folder",
        "add_project",
        "update_project",
        "remove_project",
        "project_history",
        "start_run",
        "current_run",
        "approve_draft",
        "ai_decision",
        "check_readme",
        "build_package",
        "check_assets",
        "create_assets_folder",
        "preview_release_files",
        "confirm_release_files",
        "confirm_publish",
        "cancel_run",
        "resume_run",
        "discard_run",
        "reset_working_copy",
        "vault_view",
        "save_account",
        "remove_account",
        "test_account",
        "provider_adapters",
        "list_providers",
        "save_provider",
        "remove_provider",
        "set_default_provider",
        "clear_provider_attention",
        "save_provider_fallback",
        "test_provider",
        "list_provider_models",
        "provider_fleet",
        "get_settings",
        "save_settings",
        "run_doctor",
        "svn_install_plan",
        "install_svn",
        "diagnostics",
        "check_update",
        "install_update",
    ]);
    if let Err(err) = tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
    {
        println!("cargo:warning={err}");
        std::process::exit(1);
    }
}
