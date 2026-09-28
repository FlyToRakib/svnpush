//! Step 7, Publish, and what happens after an interrupted run: Resume
//! (create only the tag) and Discard (roll back local changes).
//!
//! Every server write is journalled as in flight before it starts. When it
//! fails, or the app stops, the server is asked whether it landed, so a
//! commit that reached the server is never rolled back locally and a tag
//! that exists is recorded instead of blocking Resume.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::clock;
use crate::detect::git::Git;
use crate::detect::{self, DetectOptions};
use crate::svn::{Credentials, DEEP_FOLDERS, Svn, SvnError, TagVerification, VERIFY_DELAY};
use crate::vault;

use super::engine::Run;
use super::journal::{InFlight, Outcome, RunJournal};
use super::lock::ProjectLock;
use super::model::{Decision, ErrorView, Phase, PublishResult, RunState, Step, StepStatus};
use super::snapshot::Snapshot;
use super::{RunFailure, RunInputs, RunObserver};

/// The SVN credentials for the project's account, read from the keychain now.
pub(super) fn credentials(inputs: &RunInputs) -> Result<Credentials, RunFailure> {
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

fn tag_url(url: &str, version: &str) -> String {
    format!("{}/tags/{version}", url.trim_end_matches('/'))
}

/// The commit messages from a Publish confirmation; other decisions are not for Step 7.
pub(super) fn publish_messages(decision: Decision) -> Option<(String, String)> {
    match decision {
        Decision::Publish { trunk_message, tag_message } => Some((trunk_message, tag_message)),
        Decision::Approve { .. }
        | Decision::Generate { .. }
        | Decision::AcceptPrivacy { .. }
        | Decision::Manual
        | Decision::ApplyFixes { .. }
        | Decision::Stop
        | Decision::ConfirmFiles { .. } => None,
    }
}

impl Run {
    pub(super) async fn publish(&mut self) -> Result<(), RunFailure> {
        let (trunk_message, tag_message) =
            self.wait_for(Step::Publish, Phase::AwaitingPublish, publish_messages).await?;
        self.begin(Step::Publish, Phase::Publishing)?;
        let creds = credentials(&self.inputs)?;
        let version = self.journal.version.clone().unwrap_or_default();
        let url = self.inputs.project.svn_url.clone();
        let wc = self.inputs.paths.working_copy(&self.inputs.project.slug);
        let main_file = self.facts()?.main_file.clone();

        let folders: Vec<PathBuf> =
            DEEP_FOLDERS.iter().map(|f| wc.join(f)).filter(|p| p.is_dir()).collect();
        let trunk = self.commit_journalled(&folders, &trunk_message, &creds).await?;
        self.journal.revisions.trunk = trunk;
        self.journal.tag_message = Some(tag_message.clone());
        self.save_journal();

        // A cancel during the commit takes effect here: trunk is recorded,
        // so nothing is rolled back and Resume creates the tag.
        if self.cancel.is_cancelled() {
            return Err(RunFailure::cancelled());
        }
        self.journal.in_flight = Some(InFlight::Tag);
        self.save_journal();
        let result = self.svn().tag(&url, &version, trunk, &tag_message, &creds).await;
        let tag = match result {
            Ok(tag) => tag,
            Err(error) => {
                let fresh =
                    svn_client(&self.inputs, self.observer.as_ref(), &CancellationToken::new());
                match landed_tag(&fresh, &url, &version, &error, &creds).await {
                    Ok(Some(tag)) => tag,
                    Ok(None) if error.refused_before_write() => {
                        self.journal.in_flight = None;
                        self.save_journal();
                        return Err(error.into());
                    }
                    // Unknown (the tag may not be visible yet): the marker
                    // stays, so Resume checks again.
                    Ok(None) | Err(_) => return Err(error.into()),
                }
            }
        };
        self.journal.in_flight = None;
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
            assets_revision: None,
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

    /// Commits `folders`, journalling the commit as in flight first. When
    /// `svn` reports an error, the server's log decides whether it landed.
    /// The marker is cleared only when `svn` refused before writing
    /// anything; otherwise it stays (the commit may not be visible yet, or
    /// the log could not be read), so nothing is rolled back and Resume
    /// checks again.
    pub(super) async fn commit_journalled(
        &mut self,
        folders: &[PathBuf],
        message: &str,
        creds: &Credentials,
    ) -> Result<Option<u64>, RunFailure> {
        if self.cancel.is_cancelled() {
            return Err(RunFailure::cancelled());
        }
        let url = self.inputs.project.svn_url.clone();
        // Without the revision the commit could not be told apart from an
        // older one with the same message, so a failure here stops the run
        // before anything is written. A plugin not on the server yet has no
        // commits to confuse it with.
        let since = self.svn().last_changed_revision(&url, Some(creds)).await?.unwrap_or(0);
        if self.cancel.is_cancelled() {
            return Err(RunFailure::cancelled());
        }
        self.journal.in_flight =
            Some(InFlight::Commit { since: Some(since), message: message.to_owned() });
        self.save_journal();

        let result = self.svn().commit_paths(folders, message, since, creds).await;
        let revision = match result {
            Ok(revision) => revision,
            Err(error) => {
                let fresh =
                    svn_client(&self.inputs, self.observer.as_ref(), &CancellationToken::new());
                match fresh.find_commit(&url, since, message, Some(creds)).await {
                    Ok(Some(revision)) => {
                        self.state.notices.push(format!(
                            "svn reported an error, but the commit reached the server as r{revision}."
                        ));
                        Some(revision)
                    }
                    Ok(None) if error.refused_before_write() => {
                        self.journal.in_flight = None;
                        self.save_journal();
                        return Err(error.into());
                    }
                    // Unknown: replication lag can hide a commit that landed.
                    Ok(None) | Err(_) => return Err(error.into()),
                }
            }
        };
        self.journal.in_flight = None;
        Ok(revision)
    }

    /// Post-publish option: commit the release edits and tag `v<version>` in
    /// git. The tag is created only when the commit succeeded, or when there
    /// was nothing to commit.
    async fn git_release(&mut self, version: &str) {
        let Some(bin) = self.inputs.git.clone() else {
            self.state.notices.push("Git is not available, so no git tag was created.".to_owned());
            return;
        };
        let root = self.project_root();
        let git = Git::new(&bin, &root, self.observer.as_ref(), &self.cancel);
        let message = format!("Release {version}");
        let tag = format!("v{version}");
        let paths: Vec<&str> = self.state.diffs.iter().map(|d| d.path.as_str()).collect();
        let committed = if paths.is_empty() {
            true
        } else {
            let mut commit: Vec<&str> = vec!["commit", "-m", &message, "--"];
            commit.extend(paths);
            git.output(&commit).await.ok().flatten().is_some()
        };
        if !committed {
            self.state.notices.push(format!(
                "The git commit could not be created, so {tag} was not tagged. Commit and tag the release yourself if you need them."
            ));
            return;
        }
        if git.output(&["tag", &tag]).await.ok().flatten().is_none() {
            self.state.notices.push(format!(
                "The git tag {tag} could not be created. Create it yourself if you need it."
            ));
        }
    }
}

/// The revision of `tags/<version>` when a copy that reported `error` did
/// reach the server anyway.
async fn landed_tag(
    svn: &Svn<'_>,
    url: &str,
    version: &str,
    error: &SvnError,
    creds: &Credentials,
) -> Result<Option<u64>, SvnError> {
    // An existing tag was refused before any copy started.
    if matches!(error, SvnError::TagExists { .. }) {
        return Ok(None);
    }
    svn.last_changed_revision(&tag_url(url, version), Some(creds)).await
}

/// The main file to verify the tag with: the journal's, or detected again.
fn main_file_for(inputs: &RunInputs, journal: &RunJournal) -> Option<String> {
    journal.main_file.clone().filter(|f| !f.is_empty()).or_else(|| {
        let settings = &inputs.project.settings;
        detect::detect(
            Path::new(&journal.project_path),
            DetectOptions {
                svn_url: &inputs.project.svn_url,
                main_file: settings.main_file.as_deref(),
                version_locations: &settings.version_locations,
            },
        )
        .ok()
        .map(|facts| facts.main_file)
    })
}

/// Restores the snapshot and reverts the working copy of an interrupted run.
async fn roll_back_journal(
    inputs: &RunInputs,
    observer: &dyn RunObserver,
    journal: &RunJournal,
) -> Result<(), ErrorView> {
    if let Some(dir) = journal.snapshot.clone().filter(|d| Path::new(d).is_dir()) {
        let snapshot = Snapshot::open(Path::new(&journal.project_path), Path::new(&dir))
            .map_err(|f| f.error)?;
        let skipped = snapshot.restore().map_err(|f| f.error)?;
        observer.info("Restored the files the interrupted run changed.");
        for notice in snapshot.skipped_notices(&skipped) {
            observer.info(&notice);
        }
    }
    let wc = inputs.paths.working_copy(&journal.slug);
    if wc.join(".svn").is_dir() {
        svn_client(inputs, observer, &CancellationToken::new())
            .revert(&wc)
            .await
            .map_err(|e| ErrorView::from_coded(&e))?;
    }
    Ok(())
}

/// For a commit that was never confirmed: records it when the server has
/// it; otherwise rolls the run back and stops, since nothing was published.
async fn confirm_commit(
    inputs: &RunInputs,
    observer: &dyn RunObserver,
    svn: &Svn<'_>,
    creds: &Credentials,
    journal: &mut RunJournal,
) -> Result<(), RunFailure> {
    let (true, Some(InFlight::Commit { since, message })) =
        (journal.commit_unconfirmed(), journal.in_flight.clone())
    else {
        return Ok(());
    };
    // Only journals written before the revision was required lack it; the
    // message alone could match an older commit, so nothing is assumed.
    let Some(since) = since else {
        return Err(RunFailure::new(
            "RESUME_COMMIT_UNKNOWN",
            "SVNpush cannot tell whether the trunk commit reached the server.",
            Some("Check the plugin's SVN log. If the commit is there, create the tag yourself; if not, Discard and release again.".to_owned()),
        ));
    };
    let url = &inputs.project.svn_url;
    if let Some(revision) = svn.find_commit(url, since, &message, Some(creds)).await? {
        observer.info(&format!("The trunk commit reached the server as r{revision}."));
        journal.revisions.trunk = Some(revision);
        journal.in_flight = None;
        let _ = journal.save(&inputs.paths);
        return Ok(());
    }
    roll_back_journal(inputs, observer, journal)
        .await
        .map_err(|error| RunFailure { error, cancelled: false })?;
    journal.in_flight = None;
    Err(RunFailure::new(
        "RESUME_NOT_COMMITTED",
        "The trunk commit never reached the server, so nothing was published. Your plugin files were restored.",
        Some("Release again.".to_owned()),
    ))
}

/// The tag's revision: an existing `tags/<version>` is recorded when this
/// run's copy was in flight (it was created before the interruption), and
/// refused otherwise; without one, trunk is copied now.
async fn resume_copy(
    inputs: &RunInputs,
    observer: &dyn RunObserver,
    svn: &Svn<'_>,
    creds: &Credentials,
    journal: &mut RunJournal,
    version: &str,
) -> Result<u64, RunFailure> {
    let url = &inputs.project.svn_url;
    if let Some(existing) = svn.last_changed_revision(&tag_url(url, version), Some(creds)).await? {
        if journal.in_flight != Some(InFlight::Tag) {
            return Err(SvnError::TagExists { version: version.to_owned() }.into());
        }
        observer.info(&format!("tags/{version} already exists; checking it."));
        return Ok(existing);
    }
    let message = journal.tag_message.clone().unwrap_or_else(|| format!("Tag {version}"));
    journal.in_flight = Some(InFlight::Tag);
    let _ = journal.save(&inputs.paths);
    match svn.tag(url, version, journal.revisions.trunk, &message, creds).await {
        Ok(tag) => Ok(tag),
        Err(error) => {
            let fresh = svn_client(inputs, observer, &CancellationToken::new());
            let landed = landed_tag(&fresh, url, version, &error, creds).await;
            Ok(landed.ok().flatten().ok_or(error)?)
        }
    }
}

/// Resume after the trunk commit: create only the tag and verify it (plan
/// §8.6). A commit that was never confirmed is looked up first; a tag that
/// already exists is recorded rather than created again.
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
        confirm_commit(&inputs, observer.as_ref(), &svn, &creds, &mut journal).await?;
        let tag =
            resume_copy(&inputs, observer.as_ref(), &svn, &creds, &mut journal, &version).await?;
        journal.in_flight = None;
        journal.revisions.tag = Some(tag);
        let _ = journal.save(&inputs.paths);
        let verification = match main_file_for(&inputs, &journal) {
            Some(main_file) => {
                svn.verify_tag(&url, &version, &main_file, Some(&creds), VERIFY_DELAY).await?
            }
            None => TagVerification::Unverified(
                "The main plugin file is not known, so the tag was not checked.".to_owned(),
            ),
        };
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
                assets_revision: None,
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
/// A commit that was never confirmed is looked up first, and when it did
/// reach the server nothing is rolled back.
pub async fn discard(
    inputs: &RunInputs,
    observer: &dyn RunObserver,
    mut journal: RunJournal,
) -> Result<RunJournal, ErrorView> {
    let _lock = ProjectLock::acquire(&inputs.paths.runs(&journal.slug)).map_err(|f| f.error)?;
    // A journal without the starting revision cannot be checked (see
    // `confirm_commit`); discarding it rolls back as asked.
    if let (true, false, Some(InFlight::Commit { since: Some(since), message })) =
        (journal.commit_unconfirmed(), journal.assets_only, journal.in_flight.clone())
    {
        let svn = svn_client(inputs, observer, &CancellationToken::new());
        let landed = svn
            .find_commit(&inputs.project.svn_url, since, &message, None)
            .await
            .map_err(|e| ErrorView::from_coded(&e))?;
        if let Some(revision) = landed {
            observer.info(&format!(
                "The trunk commit had reached the server as r{revision}, so your files were kept."
            ));
            journal.revisions.trunk = Some(revision);
        }
    }
    if let (Some(InFlight::Tag), Some(version)) = (&journal.in_flight, &journal.version) {
        let svn = svn_client(inputs, observer, &CancellationToken::new());
        let url = tag_url(&inputs.project.svn_url, version);
        let landed =
            svn.last_changed_revision(&url, None).await.map_err(|e| ErrorView::from_coded(&e))?;
        if let Some(revision) = landed {
            observer.info(&format!(
                "tags/{version} had been created as r{revision}, so your files were kept."
            ));
            journal.revisions.tag = Some(revision);
        }
    }
    journal.in_flight = None;
    if journal.revisions.trunk.is_none() && journal.revisions.tag.is_none() {
        roll_back_journal(inputs, observer, &journal).await?;
    }
    if journal.outcome.is_none() {
        journal.outcome = Some(Outcome::Cancelled);
        journal.finished = Some(clock::iso8601(clock::now()));
    }
    journal.discarded = true;
    journal.save(&inputs.paths).map_err(|e| ErrorView::from_coded(&e))?;
    Ok(journal)
}
