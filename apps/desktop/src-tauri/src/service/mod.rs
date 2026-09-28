//! What the commands do, as plain functions over [`crate::state::AppState`],
//! so they can be tested without a window.

pub mod checks;
pub mod projects;
pub mod providers;
pub mod runs;
pub mod settings;
pub mod vault;

use svnpush_core::run::ErrorView;

/// Runs synchronous work (folder walks, zips, keychain calls) on the blocking
/// pool, so it never stalls the async runtime's worker threads.
pub async fn blocking<T, F>(work: F) -> Result<T, ErrorView>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, ErrorView> + Send + 'static,
{
    tokio::task::spawn_blocking(work).await.unwrap_or_else(|e| {
        Err(ErrorView::new("TASK_FAILED", format!("A background task stopped: {e}"), None))
    })
}
