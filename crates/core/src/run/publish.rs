//! Step 7, Publish, and what happens after an interrupted run: Resume
//! (create only the tag) and Discard (roll back local changes).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::clock;
use crate::detect::git::Git;
use crate::svn::{Credentials, Svn, TagVerification, VERIFY_DELAY};
use crate::vault;

use super::engine::Run;
use super::journal::{Outcome, RunJournal};
use super::lock::ProjectLock;
use super::model::{Decision, ErrorView, Phase, PublishResult, RunState, Step, StepStatus};
use super::snapshot::Snapshot;
use super::{RunFailure, RunInputs, RunObserver};

/// The SVN credentials for the project's account, read from the keychain now.
fn credentials(inputs: &RunInputs) -> Result<Credentials, RunFailure> {
    let host = vault::svn_host(&inputs.project.svn_url);
    let chosen = inputs.project.settings.svn_account.as_deref();
    let Some(account) = vault::resolve_account(&inputs.accounts, &host, chosen) else {
        return Err(RunFailure::new(
            "VAULT_NO_ACCOUNT",
            format!("No SVN account is set up for {host}."),
            Some("Open Vault, add your WordPress.org username and SVN password, and choose it in project settings if you have several.".to_owned()),
        ));
    };
    match inputs.vault.get(&account.key())? {
        Some(password) => Ok(Credentials { username: account.username.clone(), password }),
        None => Err(RunFailure::new(
            "VAULT_NO_CREDENTIALS",
            format!("No SVN password is stored for {}.", account.username),
            Some("Open Vault and save the SVN password for this account.".to_owned()),
        )),
    }
}

fn svn_client<'a>(
    inputs: &RunInputs,
    observer: &'a dyn RunObserver,
    cancel: &CancellationToken,
) -> Svn<'a> {
    let bin = inputs.svn.path.clone().map_or_else(|| PathBuf::from("svn"), PathBuf::from);
    Svn::new(bin, observer, cancel.clone())
}

impl Run {
    pub(super) async fn publish(&mut self) -> Result<(), RunFailure> {
        let (trunk_message, tag_message) = self
            .wait_for(Step::Publish, Phase::AwaitingPublish, |d| match d {
                Decision::Publish { trunk_message, tag_message } => {
                    Some((trunk_message, tag_message))
                }
                Decision::Approve { .. } => None,
            })
            .await?;
        self.begin(Step::Publish, Phase::Publishing)?;
        let creds = credentials(&self.inputs)?;
        let version = self.journal.version.clone().unwrap_or_default();
        let url = self.inputs.project.svn_url.clone();
        let wc = self.inputs.paths.working_copy(&self.inputs.project.slug);
        let main_file = self.facts()?.main_file.clone();

        let trunk = self.svn().commit(&wc, &trunk_message, &creds).await?;
        self.journal.revisions.trunk = trunk;
        self.journal.tag_message = Some(tag_message.clone());
        self.save_journal();

        let tag = self.svn().tag(&url, &version, trunk, &tag_message, &creds).await?;
        self.journal.revisions.tag = Some(tag);
        self.save_journal();

        let verification =
            self.svn().verify_tag(&url, &version, &main_file, Some(&creds), VERIFY_DELAY).await?;
        drop(creds);

        if self.inputs.project.settings.post_publish_git_tag {
            self.git_release(&version).await;
        }

        let verified = verification == TagVerification::Verified;
        self.journal.verification = Some(verification.clone());
        self.journal.outcome =
            Some(if verified { Outcome::Complete } else { Outcome::PublishedUnverified });
        self.state.publish = Some(PublishResult {
            trunk_revision: trunk,
            tag_revision: Some(tag),
            verification: Some(verification),
            plugin_url: self.inputs.project.plugin_page(),
            open_plugin_page: self.inputs.project.settings.post_publish_open_page,
            zip_path: self.package.as_ref().map(|p| p.zip_path.clone()),
        });
        let summary = match trunk {
            Some(rev) => format!("trunk r{rev} · tags/{version} r{tag}"),
            None => format!("trunk unchanged · tags/{version} r{tag}"),
        };
        self.done(Step::Publish, summary);
        self.state.phase = if verified { Phase::Verified } else { Phase::PublishedUnverified };
        if !verified {
            self.state.notices.push(
                "Published, but the tag could not be verified yet. Check the plugin page in a few minutes.".to_owned(),
            );
        }
        Ok(())
    }

    /// Post-publish option: commit the release edits and tag `v<version>` in git.
    async fn git_release(&mut self, version: &str) {
        let Some(bin) = self.inputs.git.clone() else {
            self.state.notices.push("Git is not available, so no git tag was created.".to_owned());
            return;
        };
        let root = self.project_root();
        let git = Git::new(&bin, &root, self.observer.as_ref(), &self.cancel);
        let message = format!("Release {version}");
        let mut commit: Vec<&str> = vec!["commit", "-m", &message, "--"];
        commit.extend(self.state.diffs.iter().map(|d| d.path.as_str()));
        let tag = format!("v{version}");
        let committed = git.output(&commit).await.ok().flatten().is_some();
        let tagged = git.output(&["tag", &tag]).await.ok().flatten().is_some();
        if !(committed && tagged) {
            self.state.notices.push(format!(
                "The git commit or tag {tag} could not be created. Create them yourself if you need them."
            ));
        }
    }
}

/// Resume after the trunk commit: create only the tag and verify it (plan §8.6).
pub async fn resume_tag(
    inputs: RunInputs,
    observer: Arc<dyn RunObserver>,
    cancel: CancellationToken,
    mut journal: RunJournal,
) -> (RunState, RunJournal) {
    let mut state = RunState::new(journal.id.clone(), journal.project_path.clone(), false);
    for step in Step::ALL {
        state.set_step(step, StepStatus::Done, None);
    }
    state.phase = Phase::Publishing;
    state.set_step(
        Step::Publish,
        StepStatus::Running,
        Some("Resuming: creating the tag".to_owned()),
    );
    observer.state(&state);

    let result = async {
        let _lock = ProjectLock::acquire(&inputs.paths.runs(&journal.slug))?;
        let version = journal.version.clone().ok_or_else(|| {
            RunFailure::new("RESUME_NO_VERSION", "The journal has no version.", None)
        })?;
        let creds = credentials(&inputs)?;
        let svn = svn_client(&inputs, observer.as_ref(), &cancel);
        let url = inputs.project.svn_url.clone();
        let message = journal.tag_message.clone().unwrap_or_else(|| format!("Tag {version}"));
        let tag = svn.tag(&url, &version, journal.revisions.trunk, &message, &creds).await?;
        journal.revisions.tag = Some(tag);
        let _ = journal.save(&inputs.paths);
        let main_file = journal.main_file.clone().unwrap_or_default();
        let verification =
            svn.verify_tag(&url, &version, &main_file, Some(&creds), VERIFY_DELAY).await?;
        Ok::<_, RunFailure>((version, tag, verification))
    }
    .await;

    match result {
        Ok((version, tag, verification)) => {
            let verified = verification == TagVerification::Verified;
            journal.outcome =
                Some(if verified { Outcome::Complete } else { Outcome::PublishedUnverified });
            journal.verification = Some(verification.clone());
            state.phase = if verified { Phase::Verified } else { Phase::PublishedUnverified };
            state.set_step(Step::Publish, StepStatus::Done, Some(format!("tags/{version} r{tag}")));
            state.publish = Some(PublishResult {
                trunk_revision: journal.revisions.trunk,
                tag_revision: Some(tag),
                verification: Some(verification),
                plugin_url: inputs.project.plugin_page(),
                open_plugin_page: inputs.project.settings.post_publish_open_page,
                zip_path: None,
            });
        }
        Err(failure) => {
            state.phase = if failure.cancelled { Phase::Cancelled } else { Phase::Failed };
            state.set_step(Step::Publish, StepStatus::Failed, Some(failure.error.message.clone()));
            state.error = (!failure.cancelled).then_some(failure.error);
            journal.outcome = Some(Outcome::Failed(Step::Publish));
        }
    }
    journal.finished = Some(clock::iso8601(clock::now()));
    if let Err(e) = journal.save(&inputs.paths) {
        state.notices.push(format!("Could not save the run journal: {e}"));
    }
    observer.state(&state);
    (state, journal)
}

/// Discard an interrupted run: restore the snapshot (when trunk was not
/// committed), revert the working copy, and close the journal as cancelled.
pub async fn discard(
    inputs: &RunInputs,
    observer: &dyn RunObserver,
    mut journal: RunJournal,
) -> Result<RunJournal, ErrorView> {
    let _lock = ProjectLock::acquire(&inputs.paths.runs(&journal.slug)).map_err(|f| f.error)?;
    if journal.revisions.trunk.is_none() {
        if let Some(dir) = journal.snapshot.clone().filter(|d| Path::new(d).is_dir()) {
            let snapshot = Snapshot::open(Path::new(&journal.project_path), Path::new(&dir))
                .map_err(|f| f.error)?;
            snapshot.restore().map_err(|f| f.error)?;
            observer.info("Restored the files the interrupted run changed.");
        }
        let wc = inputs.paths.working_copy(&journal.slug);
        if wc.join(".svn").is_dir() {
            svn_client(inputs, observer, &CancellationToken::new())
                .revert(&wc)
                .await
                .map_err(|e| ErrorView::from_coded(&e))?;
        }
    }
    if journal.outcome.is_none() {
        journal.outcome = Some(Outcome::Cancelled);
        journal.finished = Some(clock::iso8601(clock::now()));
    }
    journal.discarded = true;
    journal.save(&inputs.paths).map_err(|e| ErrorView::from_coded(&e))?;
    Ok(journal)
}
