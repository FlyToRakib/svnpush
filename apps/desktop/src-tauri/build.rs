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
        "confirm_publish",
        "cancel_run",
        "resume_run",
        "discard_run",
        "reset_working_copy",
    ]);
    if let Err(err) = tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
    {
        println!("cargo:warning={err}");
        std::process::exit(1);
    }
}
