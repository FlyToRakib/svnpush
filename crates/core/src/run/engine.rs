//! Driving a run through its steps, and what happens when it stops.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::clock;
use crate::package::Package;
use crate::svn::Svn;

use super::journal::{Outcome, RunJournal};
use super::lock::ProjectLock;
use super::model::{Decision, Phase, RunState, Step, StepStatus};
use super::snapshot::Snapshot;
use super::{RunFailure, RunInputs, RunObserver};

/// One release run. Create it with [`Run::prepare`], drive it with [`Run::execute`].
pub struct Run {
    pub(super) inputs: RunInputs,
    pub(super) observer: Arc<dyn RunObserver>,
    pub(super) cancel: CancellationToken,
    pub(super) decisions: mpsc::Receiver<Decision>,
    pub(super) state: RunState,
    pub(super) journal: RunJournal,
    pub(super) snapshot: Option<Snapshot>,
    pub(super) package: Option<Package>,
    pub(super) previewed: bool,
    /// The provider chosen with the Change link; it applies to the rest of the run.
    pub(super) ai_override: Option<String>,
    _lock: ProjectLock,
}

impl Run {
    /// Takes the project lock and creates the journal. Fails when another run holds the lock.
    pub fn prepare(
        inputs: RunInputs,
        observer: Arc<dyn RunObserver>,
        cancel: CancellationToken,
        decisions: mpsc::Receiver<Decision>,
    ) -> Result<Self, RunFailure> {
        let slug = inputs.project.slug.clone();
        let lock = ProjectLock::acquire(&inputs.paths.runs(&slug))?;
        let id = clock::file_stamp(clock::now());
        let state = RunState::new(id.clone(), inputs.project.path.clone(), inputs.dry_run);
        let journal = RunJournal::start(&id, &slug, &inputs.project.path, inputs.dry_run);
        Ok(Self {
            inputs,
            observer,
            cancel,
            decisions,
            state,
            journal,
            snapshot: None,
            package: None,
            previewed: false,
            ai_override: None,
            _lock: lock,
        })
    }

    /// The initial state, before [`Run::execute`] starts.
    pub fn state(&self) -> &RunState {
        &self.state
    }

    /// Runs every step and returns the final state and journal.
    pub async fn execute(mut self) -> (RunState, RunJournal) {
        self.save_journal();
        let result = self.all_steps().await;
        match result {
            Ok(()) => {}
            Err(failure) => self.stop(failure).await,
        }
        self.journal.finished = Some(clock::iso8601(clock::now()));
        self.save_journal();
        self.emit();
        (self.state, self.journal)
    }

    async fn all_steps(&mut self) -> Result<(), RunFailure> {
        self.detect().await?;
        self.draft().await?;
        self.write()?;
        self.verify().await?;
        self.build().await?;
        self.preview().await?;
        if self.inputs.dry_run {
            return self.finish_dry_run().await;
        }
        self.publish().await
    }

    pub(super) fn svn(&self) -> Svn<'_> {
        let bin = self.inputs.svn.path.clone().map_or_else(|| PathBuf::from("svn"), PathBuf::from);
        Svn::new(bin, self.observer.as_ref(), self.cancel.clone())
    }

    pub(super) fn emit(&self) {
        self.observer.state(&self.state);
    }

    pub(super) fn save_journal(&self) {
        if let Err(e) = self.journal.save(&self.inputs.paths) {
            self.observer.info(&format!("Could not save the run journal: {e}"));
        }
    }

    pub(super) fn begin(&mut self, step: Step, phase: Phase) -> Result<(), RunFailure> {
        if self.cancel.is_cancelled() {
            return Err(RunFailure::cancelled());
        }
        self.state.phase = phase;
        self.state.set_step(step, StepStatus::Running, None);
        self.emit();
        Ok(())
    }

    pub(super) fn done(&mut self, step: Step, summary: String) {
        self.state.set_step(step, StepStatus::Done, Some(summary.clone()));
        self.journal.record(step, StepStatus::Done, Some(summary));
        self.save_journal();
        self.emit();
    }

    /// Waits for the developer, ignoring answers meant for another step.
    pub(super) async fn wait_for<T>(
        &mut self,
        step: Step,
        phase: Phase,
        accept: impl Fn(Decision) -> Option<T>,
    ) -> Result<T, RunFailure> {
        self.state.phase = phase;
        self.state.set_step(step, StepStatus::Waiting, None);
        self.emit();
        loop {
            tokio::select! {
                () = self.cancel.cancelled() => return Err(RunFailure::cancelled()),
                decision = self.decisions.recv() => match decision {
                    Some(d) => if let Some(value) = accept(d) { return Ok(value) },
                    None => return Err(RunFailure::cancelled()),
                },
            }
        }
    }

    /// Undoes local changes: restores the snapshot, reverts the working copy
    /// and, when asked, removes the staged build. Errors become notices.
    pub(super) async fn roll_back(&mut self, remove_build: bool) {
        if let Some(snapshot) = &self.snapshot {
            match snapshot.restore() {
                Ok(()) => self.observer.info("Restored the files Step 3 changed."),
                Err(e) => self
                    .state
                    .notices
                    .push(format!("Could not restore changed files: {}", e.error.message)),
            }
        }
        if self.previewed {
            let wc = self.inputs.paths.working_copy(&self.inputs.project.slug);
            // A fresh token: the run's own token is already cancelled when
            // rolling back after Cancel, and the revert must still happen.
            let bin =
                self.inputs.svn.path.clone().map_or_else(|| PathBuf::from("svn"), PathBuf::from);
            let svn = Svn::new(bin, self.observer.as_ref(), CancellationToken::new());
            if let Err(e) = svn.revert(&wc).await {
                self.state.notices.push(format!("Could not revert the working copy: {e}"));
            }
        }
        let build_dir = self
            .package
            .as_ref()
            .and_then(|p| PathBuf::from(&p.zip_path).parent().map(PathBuf::from));
        if remove_build
            && let Some(dir) = build_dir.filter(|d| d.exists())
            && let Err(e) = std::fs::remove_dir_all(&dir)
        {
            self.state.notices.push(format!("Could not remove the staged build: {e}"));
        }
    }

    async fn stop(&mut self, failure: RunFailure) {
        let step = self.state.active_step().unwrap_or(Step::Detect);
        if self.journal.revisions.trunk.is_some() {
            self.state.notices.push(
                "Trunk was already committed and cannot be undone. Use Resume to create the tag."
                    .to_owned(),
            );
        } else {
            let remove_build =
                failure.cancelled && matches!(step, Step::Build | Step::Preview | Step::Publish);
            self.roll_back(remove_build).await;
        }
        if failure.cancelled {
            self.state.phase = Phase::Cancelled;
            self.state.set_step(step, StepStatus::Failed, Some("Cancelled".to_owned()));
            self.journal.record(step, StepStatus::Failed, Some("Cancelled".to_owned()));
            self.journal.outcome = Some(Outcome::Cancelled);
        } else {
            self.state.phase = Phase::Failed;
            self.state.set_step(step, StepStatus::Failed, Some(failure.error.message.clone()));
            self.journal.record(step, StepStatus::Failed, Some(failure.error.message.clone()));
            self.journal.outcome = Some(Outcome::Failed(step));
        }
        self.state.error = (!failure.cancelled).then_some(failure.error);
    }

    async fn finish_dry_run(&mut self) -> Result<(), RunFailure> {
        self.roll_back(false).await;
        self.snapshot = None;
        self.state.set_step(
            Step::Publish,
            StepStatus::Skipped,
            Some("Dry run: nothing was committed.".to_owned()),
        );
        self.state.phase = Phase::DryRunComplete;
        self.state.notices.push(
            "Dry run complete. Your plugin files and the working copy are unchanged.".to_owned(),
        );
        self.journal.outcome = Some(Outcome::DryRun);
        Ok(())
    }
}
