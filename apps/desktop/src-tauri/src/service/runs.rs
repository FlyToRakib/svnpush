//! Runs: start, answer, cancel, resume, discard, and working-copy actions.

use std::path::PathBuf;
use std::sync::Arc;

use svnpush_core::package::Exclusions;
use svnpush_core::report::NullReporter;
use svnpush_core::run::files::FilePreview;
use svnpush_core::run::{
    self, Decision, ErrorView, Phase, ProjectLock, ReleaseDraft, Run, RunInputs, RunJournal,
    RunObserver, RunState, journal,
};
use svnpush_core::svn::Svn;
use svnpush_core::{project, settings, tools, vault};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::events::{EventSink, ProjectObserver};
use crate::service::{self, projects};
use crate::state::{AppState, RunControl, RunSlot};

async fn inputs(app: &AppState, path: &str, dry_run: bool) -> Result<RunInputs, ErrorView> {
    let project = projects::load(app, path).await?;
    let config = settings::load(&app.paths).map_err(|e| ErrorView::from_coded(&e))?;
    let svn = tools::discover_svn(config.svn_path.as_deref().map(std::path::Path::new)).await;
    let git = tools::discover_git(config.git_path.as_deref().map(std::path::Path::new)).await;
    let accounts = vault::load_accounts(&app.paths).map_err(|e| ErrorView::from_coded(&e))?;
    let current_wordpress = settings::current_wordpress_version(&app.paths, &config).await;
    Ok(RunInputs {
        paths: app.paths.clone(),
        project,
        dry_run,
        git: git.ok.then(|| git.path.map(PathBuf::from)).flatten(),
        svn,
        vault: app.vault.clone(),
        accounts,
        current_wordpress,
        ai: app.ai.clone(),
        assets_only: false,
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

fn already_active() -> ErrorView {
    ErrorView::new(
        "RUN_ALREADY_ACTIVE",
        "A release is already running for this plugin.",
        Some("Finish or cancel it first.".to_owned()),
    )
}

/// Claims the project's slot for a new run before anything is awaited, so a
/// second start is refused from this moment. Returns the slot it replaced.
fn claim(
    app: &AppState,
    path: &str,
    state: RunState,
    control: RunControl,
) -> Result<Option<RunSlot>, ErrorView> {
    let mut runs = app.runs();
    if runs.get(path).is_some_and(|slot| slot.control.is_some()) {
        return Err(already_active());
    }
    Ok(runs.insert(path.to_owned(), RunSlot { state, control: Some(control) }))
}

/// Gives the slot back after a claimed run failed to start.
fn unclaim(app: &AppState, path: &str, previous: Option<RunSlot>) {
    let mut runs = app.runs();
    match previous {
        Some(slot) => runs.insert(path.to_owned(), slot),
        None => runs.remove(path),
    };
}

/// Starts a release, an assets-only release, or a dry run of either, and returns its first state.
pub async fn start(
    app: Arc<AppState>,
    sink: Arc<dyn EventSink>,
    path: &str,
    dry_run: bool,
    assets_only: bool,
) -> Result<RunState, ErrorView> {
    let (decisions, receiver) = mpsc::channel(4);
    let cancel = CancellationToken::new();
    let placeholder = RunState::new(String::new(), path.to_owned(), dry_run);
    let previous =
        claim(&app, path, placeholder, RunControl { decisions, cancel: cancel.clone() })?;
    let prepared = async {
        let mut inputs = inputs(&app, path, dry_run).await?;
        inputs.assets_only = assets_only;
        let watcher = observer(&app, &sink, path);
        Run::prepare(inputs, watcher, cancel, receiver).map_err(|f| f.error)
    }
    .await;
    let run = match prepared {
        Ok(run) => run,
        Err(error) => {
            unclaim(&app, path, previous);
            return Err(error);
        }
    };
    let first = run.state().clone();
    if let Some(slot) = app.runs().get_mut(path) {
        slot.state = first.clone();
    }

    let owned_path = path.to_owned();
    tauri::async_runtime::spawn(async move {
        let (last, journal) = Box::pin(run.execute()).await;
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
    // Only the run that owns the slot may end it: a stale run must never
    // clear the controls of the run that replaced it.
    let owned = match app.runs().get_mut(path) {
        Some(slot) if slot.state.id == last.id => {
            slot.state = last.clone();
            slot.control = None;
            true
        }
        _ => false,
    };
    if owned {
        sink.run_state(crate::events::RunStateEvent { project_path: path.to_owned(), state: last });
    }
}

/// The latest run state for a project, if it has run since the app started.
pub fn current(app: &AppState, path: &str) -> Option<RunState> {
    app.runs().get(path).map(|slot| slot.state.clone())
}

/// Step 2: approves a draft after validating it.
pub async fn approve(app: &AppState, path: &str, draft: ReleaseDraft) -> Result<(), ErrorView> {
    run::validate_draft(&draft)?;
    let control = control(app, path)?;
    // Approving while the AI is still drafting stops the AI and uses this draft.
    let waiting = app.runs().get(path).is_some_and(|s| {
        s.state.phase == Phase::AwaitingApproval
            || (s.state.phase == Phase::Drafting && s.state.draft_context.is_some())
    });
    if !waiting {
        return Err(ErrorView::new(
            "RUN_NOT_WAITING",
            "The release is not waiting for a draft.",
            None,
        ));
    }
    control.decisions.send(Decision::Approve { draft }).await.map_err(|_| no_run(path))
}

/// What `distignore` would release from the project's package root, for the
/// Step 5 file check while the developer edits the rules.
pub async fn preview_files(
    app: &AppState,
    path: &str,
    distignore: &str,
) -> Result<FilePreview, ErrorView> {
    let project = projects::load(app, path).await?;
    let root = project.package_root();
    if !root.is_dir() {
        return Err(ErrorView::new(
            "PACKAGE_ROOT_MISSING",
            format!("The package root {} does not exist.", root.display()),
            Some("Check the package root in project settings.".to_owned()),
        ));
    }
    let distignore = distignore.to_owned();
    let skip = [app.paths.builds()];
    service::blocking(move || {
        run::files::preview(&root, &distignore, &skip).map_err(|e| ErrorView::from_coded(&e))
    })
    .await
}

/// Step 5: the files to release are right. `distignore`, when given, is saved
/// as the project's `.distignore` first.
pub async fn confirm_files(
    app: &AppState,
    path: &str,
    distignore: Option<String>,
) -> Result<(), ErrorView> {
    if let Some(text) = &distignore {
        let project = projects::load(app, path).await?;
        Exclusions::from_text(&project.package_root(), text, &[])
            .map_err(|e| ErrorView::from_coded(&e))?;
    }
    let control = control(app, path)?;
    let waiting = app.runs().get(path).is_some_and(|s| s.state.phase == Phase::AwaitingFileReview);
    if !waiting {
        return Err(ErrorView::new(
            "RUN_NOT_WAITING",
            "The release is not waiting for the file check.",
            None,
        ));
    }
    control.decisions.send(Decision::ConfirmFiles { distignore }).await.map_err(|_| no_run(path))
}

/// Step 2 and Step 4 AI actions: generate (or Change provider), accept the
/// privacy notice, write by hand, apply suggested fixes, stop.
pub async fn ai_decision(app: &AppState, path: &str, decision: Decision) -> Result<(), ErrorView> {
    if matches!(
        decision,
        Decision::Approve { .. } | Decision::ConfirmFiles { .. } | Decision::Publish { .. }
    ) {
        return Err(ErrorView::new(
            "RUN_WRONG_DECISION",
            "Approve, the file check and Publish have their own confirmations.",
            None,
        ));
    }
    let control = control(app, path)?;
    let waiting = app.runs().get(path).is_some_and(|s| {
        matches!(
            s.state.phase,
            Phase::Drafting | Phase::AwaitingApproval | Phase::Verifying | Phase::AwaitingFixes
        )
    });
    if !waiting {
        return Err(ErrorView::new(
            "RUN_NOT_WAITING",
            "The release is not at a step that uses AI.",
            None,
        ));
    }
    control.decisions.send(decision).await.map_err(|_| no_run(path))
}

/// Step 7: the explicit Publish confirmation.
pub async fn publish(
    app: &AppState,
    path: &str,
    trunk_message: String,
    tag_message: String,
) -> Result<(), ErrorView> {
    let assets_only = app.runs().get(path).is_some_and(|s| s.state.assets_only);
    if trunk_message.trim().is_empty() || (!assets_only && tag_message.trim().is_empty()) {
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
    if app.is_active(path) {
        return Err(already_active());
    }
    let found = journal_by_id(&app, path, id).await?;
    if found.needs_tag() {
        let cancel = CancellationToken::new();
        let (decisions, _) = mpsc::channel(1);
        let mut first = RunState::new(found.id.clone(), path.to_owned(), false);
        first.phase = Phase::Publishing;
        let previous =
            claim(&app, path, first.clone(), RunControl { decisions, cancel: cancel.clone() })?;
        let inputs = match inputs(&app, path, false).await {
            Ok(inputs) => inputs,
            Err(error) => {
                unclaim(&app, path, previous);
                return Err(error);
            }
        };
        let watcher = observer(&app, &sink, path);
        let owned_path = path.to_owned();
        tauri::async_runtime::spawn(async move {
            let (last, journal) = run::resume_tag(inputs, watcher, cancel, found).await;
            finish(&app, &sink, &owned_path, last, &journal).await;
        });
        return Ok(first);
    }
    discard(&app, &sink, path, id).await?;
    start(app, sink, path, found.dry_run, found.assets_only).await
}

/// Discards an unfinished run, restoring files and the working copy.
/// Refused while a run is active: a resume that is starting would save its
/// journal over the discarded one.
pub async fn discard(
    app: &Arc<AppState>,
    sink: &Arc<dyn EventSink>,
    path: &str,
    id: &str,
) -> Result<(), ErrorView> {
    if app.is_active(path) {
        return Err(already_active());
    }
    let found = journal_by_id(app, path, id).await?;
    let inputs = inputs(app, path, false).await?;
    let watcher: Arc<dyn RunObserver> = observer(app, sink, path);
    run::discard(&inputs, watcher.as_ref(), found).await.map(|_| ())
}

/// Deletes and re-checks-out the sparse working copy (the V15 fix). Refused
/// while a release runs, and holds the project lock throughout.
pub async fn reset_working_copy(app: &AppState, path: &str) -> Result<(), ErrorView> {
    if app.is_active(path) {
        return Err(ErrorView::new(
            "RUN_ALREADY_ACTIVE",
            "A release is running for this plugin.",
            Some("Wait for it to finish, then reset the working copy.".to_owned()),
        ));
    }
    let inputs = inputs(app, path, true).await?;
    let Some(bin) = inputs.svn.path.clone().filter(|_| inputs.svn.ok) else {
        return Err(ErrorView::new("TOOLS_SVN_UNAVAILABLE", inputs.svn.message, inputs.svn.fix));
    };
    let _lock = ProjectLock::acquire(&app.paths.runs(&inputs.project.slug)).map_err(|f| f.error)?;
    let wc = app.paths.working_copy(&inputs.project.slug);
    Svn::new(PathBuf::from(bin), &NullReporter, CancellationToken::new())
        .reset(&inputs.project.svn_url, &wc, None)
        .await
        .map_err(|e| ErrorView::from_coded(&e))
}

/// Deletes snapshots of successful publishes older than seven days, for every project.
pub async fn prune(app: &AppState) {
    if let Ok(list) = projects::list(app).await {
        for summary in list {
            // Runs keep their journals under the slug of the team file's URL.
            let slug = project::with_team_config(&summary.project)
                .map_or(summary.project.slug, |effective| effective.slug);
            let _ = journal::prune_snapshots(&app.paths, &slug);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{self, RecordingSink};

    fn control() -> RunControl {
        let (decisions, _) = mpsc::channel(1);
        RunControl { decisions, cancel: CancellationToken::new() }
    }

    #[test]
    fn a_claimed_slot_refuses_a_second_start_until_it_is_given_back() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_support::app(dir.path());
        let state = || RunState::new(String::new(), "/p".into(), false);
        let previous = claim(&app, "/p", state(), control()).unwrap();
        assert!(previous.is_none());
        assert!(app.is_active("/p"));
        assert_eq!(claim(&app, "/p", state(), control()).unwrap_err().code, "RUN_ALREADY_ACTIVE");
        unclaim(&app, "/p", previous);
        assert!(app.runs().get("/p").is_none());

        // A failed start puts the finished run's slot back as it was.
        app.runs().insert("/p".into(), RunSlot { state: state(), control: None });
        let previous = claim(&app, "/p", state(), control()).unwrap();
        unclaim(&app, "/p", previous);
        assert!(app.runs().get("/p").is_some_and(|s| s.control.is_none()));
    }

    #[tokio::test]
    async fn only_the_run_that_owns_the_slot_ends_it() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_support::app(dir.path());
        let recorder = Arc::new(RecordingSink::default());
        let sink: Arc<dyn EventSink> = recorder.clone();
        test_support::active_run(&app, "/p", "live");

        let stale = RunState::new("stale".into(), "/p".into(), false);
        let journal = RunJournal::start("stale", "p", "/p", false);
        finish(&app, &sink, "/p", stale, &journal).await;
        assert!(app.is_active("/p"));
        assert!(recorder.0.lock().unwrap().is_empty());

        let live = RunState::new("live".into(), "/p".into(), false);
        let journal = RunJournal::start("live", "p", "/p", false);
        finish(&app, &sink, "/p", live, &journal).await;
        assert!(!app.is_active("/p"));
        assert_eq!(recorder.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn ai_decision_refuses_the_decisions_with_their_own_commands() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_support::app(dir.path());
        test_support::active_run(&app, "/p", "live");
        app.runs().get_mut("/p").unwrap().state.phase = Phase::Verifying;
        let files = Decision::ConfirmFiles { distignore: Some("[".into()) };
        assert_eq!(ai_decision(&app, "/p", files).await.unwrap_err().code, "RUN_WRONG_DECISION");
        let publish = Decision::Publish { trunk_message: "t".into(), tag_message: "t".into() };
        assert_eq!(ai_decision(&app, "/p", publish).await.unwrap_err().code, "RUN_WRONG_DECISION");
    }

    #[tokio::test]
    async fn resume_discard_and_reset_are_refused_while_a_run_is_active() {
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(test_support::app(dir.path()));
        let sink: Arc<dyn EventSink> = Arc::new(RecordingSink::default());
        test_support::active_run(&app, "/p", "live");
        let resumed = resume(app.clone(), sink.clone(), "/p", "old").await.unwrap_err();
        assert_eq!(resumed.code, "RUN_ALREADY_ACTIVE");
        let discarded = discard(&app, &sink, "/p", "old").await.unwrap_err();
        assert_eq!(discarded.code, "RUN_ALREADY_ACTIVE");
        let reset = reset_working_copy(&app, "/p").await.unwrap_err();
        assert_eq!(reset.code, "RUN_ALREADY_ACTIVE");
        assert!(app.is_active("/p"));
    }
}
