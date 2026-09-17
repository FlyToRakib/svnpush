//! Runs: start, answer, cancel, resume, discard, and working-copy actions.

use std::path::PathBuf;
use std::sync::Arc;

use svnpush_core::report::NullReporter;
use svnpush_core::run::{
    self, Decision, ErrorView, Phase, ReleaseDraft, Run, RunInputs, RunJournal, RunObserver,
    RunState, journal,
};
use svnpush_core::svn::Svn;
use svnpush_core::{tools, vault};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::events::{EventSink, ProjectObserver};
use crate::service::projects;
use crate::state::{AppState, RunControl, RunSlot};

async fn inputs(app: &AppState, path: &str, dry_run: bool) -> Result<RunInputs, ErrorView> {
    let project = projects::load(app, path).await?;
    let svn = tools::discover_svn(None).await;
    let git = tools::discover_git(None).await;
    let accounts = vault::load_accounts(&app.paths).map_err(|e| ErrorView::from_coded(&e))?;
    Ok(RunInputs {
        paths: app.paths.clone(),
        project,
        dry_run,
        git: git.ok.then(|| git.path.map(PathBuf::from)).flatten(),
        svn,
        vault: app.vault.clone(),
        accounts,
        current_wordpress: None,
    })
}

fn no_run(path: &str) -> ErrorView {
    ErrorView::new("RUN_NOT_ACTIVE", format!("No release is running for {path}."), None)
}

fn control(app: &AppState, path: &str) -> Result<RunControl, ErrorView> {
    app.runs().get(path).and_then(|slot| slot.control.clone()).ok_or_else(|| no_run(path))
}

fn observer(app: &Arc<AppState>, sink: &Arc<dyn EventSink>, path: &str) -> Arc<ProjectObserver> {
    Arc::new(ProjectObserver {
        project_path: path.to_owned(),
        app: app.clone(),
        sink: sink.clone(),
    })
}

/// Starts a release or dry run and returns its first state.
pub async fn start(
    app: Arc<AppState>,
    sink: Arc<dyn EventSink>,
    path: &str,
    dry_run: bool,
) -> Result<RunState, ErrorView> {
    if app.runs().get(path).is_some_and(|slot| slot.control.is_some()) {
        return Err(ErrorView::new(
            "RUN_ALREADY_ACTIVE",
            "A release is already running for this plugin.",
            Some("Finish or cancel it first.".to_owned()),
        ));
    }
    let inputs = inputs(&app, path, dry_run).await?;
    let (decisions, receiver) = mpsc::channel(4);
    let cancel = CancellationToken::new();
    let watcher = observer(&app, &sink, path);
    let run = Run::prepare(inputs, watcher, cancel.clone(), receiver).map_err(|f| f.error)?;
    let first = run.state().clone();
    app.runs().insert(
        path.to_owned(),
        RunSlot { state: first.clone(), control: Some(RunControl { decisions, cancel }) },
    );

    let owned_path = path.to_owned();
    tauri::async_runtime::spawn(async move {
        let (last, journal) = run.execute().await;
        finish(&app, &sink, &owned_path, last, &journal).await;
    });
    Ok(first)
}

async fn finish(
    app: &AppState,
    sink: &Arc<dyn EventSink>,
    path: &str,
    last: RunState,
    journal: &RunJournal,
) {
    let mut last = last;
    if let Err(error) = projects::record_outcome(app, path, journal).await {
        last.notices.push(format!("Could not save the result on the project: {}", error.message));
    }
    if let Some(slot) = app.runs().get_mut(path) {
        slot.state = last.clone();
        slot.control = None;
    }
    sink.run_state(crate::events::RunStateEvent { project_path: path.to_owned(), state: last });
}

/// The latest run state for a project, if it has run since the app started.
pub fn current(app: &AppState, path: &str) -> Option<RunState> {
    app.runs().get(path).map(|slot| slot.state.clone())
}

/// Step 2: approves a draft after validating it.
pub async fn approve(app: &AppState, path: &str, draft: ReleaseDraft) -> Result<(), ErrorView> {
    run::validate_draft(&draft)?;
    let control = control(app, path)?;
    let waiting = app.runs().get(path).is_some_and(|s| s.state.phase == Phase::AwaitingApproval);
    if !waiting {
        return Err(ErrorView::new(
            "RUN_NOT_WAITING",
            "The release is not waiting for a draft.",
            None,
        ));
    }
    control.decisions.send(Decision::Approve { draft }).await.map_err(|_| no_run(path))
}

/// Step 7: the explicit Publish confirmation.
pub async fn publish(
    app: &AppState,
    path: &str,
    trunk_message: String,
    tag_message: String,
) -> Result<(), ErrorView> {
    if trunk_message.trim().is_empty() || tag_message.trim().is_empty() {
        return Err(ErrorView::new(
            "PUBLISH_EMPTY_MESSAGE",
            "Commit messages cannot be empty.",
            Some("Write a message for the trunk commit and the tag.".to_owned()),
        ));
    }
    let control = control(app, path)?;
    let waiting = app.runs().get(path).is_some_and(|s| s.state.phase == Phase::AwaitingPublish);
    if !waiting {
        return Err(ErrorView::new(
            "RUN_NOT_WAITING",
            "The release is not waiting for confirmation.",
            None,
        ));
    }
    control
        .decisions
        .send(Decision::Publish { trunk_message, tag_message })
        .await
        .map_err(|_| no_run(path))
}

/// Cancels the running release.
pub fn cancel(app: &AppState, path: &str) -> Result<(), ErrorView> {
    control(app, path)?.cancel.cancel();
    Ok(())
}

async fn journal_by_id(app: &AppState, path: &str, id: &str) -> Result<RunJournal, ErrorView> {
    projects::history(app, path).await?.into_iter().find(|j| j.id == id).ok_or_else(|| {
        ErrorView::new("RUN_JOURNAL_NOT_FOUND", format!("No run {id} for this project."), None)
    })
}

/// Resume an unfinished run: create only the tag when trunk was committed;
/// otherwise roll the interrupted run back and start a fresh one.
pub async fn resume(
    app: Arc<AppState>,
    sink: Arc<dyn EventSink>,
    path: &str,
    id: &str,
) -> Result<RunState, ErrorView> {
    let found = journal_by_id(&app, path, id).await?;
    if found.needs_tag() {
        let inputs = inputs(&app, path, false).await?;
        let watcher = observer(&app, &sink, path);
        let cancel = CancellationToken::new();
        let (decisions, _) = mpsc::channel(1);
        let mut first = RunState::new(found.id.clone(), path.to_owned(), false);
        first.phase = Phase::Publishing;
        app.runs().insert(
            path.to_owned(),
            RunSlot {
                state: first.clone(),
                control: Some(RunControl { decisions, cancel: cancel.clone() }),
            },
        );
        let owned_path = path.to_owned();
        tauri::async_runtime::spawn(async move {
            let (last, journal) = run::resume_tag(inputs, watcher, cancel, found).await;
            finish(&app, &sink, &owned_path, last, &journal).await;
        });
        return Ok(first);
    }
    discard(&app, &sink, path, id).await?;
    start(app, sink, path, false).await
}

/// Discards an unfinished run, restoring files and the working copy.
pub async fn discard(
    app: &Arc<AppState>,
    sink: &Arc<dyn EventSink>,
    path: &str,
    id: &str,
) -> Result<(), ErrorView> {
    let found = journal_by_id(app, path, id).await?;
    let inputs = inputs(app, path, false).await?;
    let watcher: Arc<dyn RunObserver> = observer(app, sink, path);
    run::discard(&inputs, watcher.as_ref(), found).await.map(|_| ())
}

fn svn_client() -> Svn<'static> {
    let bin = tools::find_on_path("svn").unwrap_or_else(|| PathBuf::from("svn"));
    Svn::new(bin, &NullReporter, CancellationToken::new())
}

/// Deletes and re-checks-out the sparse working copy (the V15 fix).
pub async fn reset_working_copy(app: &AppState, path: &str) -> Result<(), ErrorView> {
    let project = projects::load(app, path).await?;
    let wc = app.paths.working_copy(&project.slug);
    svn_client().reset(&project.svn_url, &wc, None).await.map_err(|e| ErrorView::from_coded(&e))
}

/// Deletes snapshots of successful publishes older than seven days, for every project.
pub async fn prune(app: &AppState) {
    if let Ok(list) = projects::list(app).await {
        for summary in list {
            let _ = journal::prune_snapshots(&app.paths, &summary.project.slug);
        }
    }
}
