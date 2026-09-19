//! The assets-only release (plan §8.7): sync `assets/` from the project's
//! assets folder and commit it alone, with no version change and no tag.
//! It uses the same checklist, preview and Publish confirmation.

use crate::edit;
use crate::readme::README_FILE;
use crate::svn::{self, Delta};
use crate::verify::{self, VerifyInput, WorkingCopyState};
use crate::wporg_assets;

use super::RunFailure;
use super::engine::Run;
use super::journal::Outcome;
use super::model::{Phase, PublishResult, Step, StepStatus, SvnPreview};
use super::publish::{credentials, publish_messages};

/// The checks that apply when only `assets/` is committed.
pub const ASSETS_CHECKS: [&str; 3] = ["V14", "V16", "W04"];

const SKIPPED: &str = "Not part of an assets-only release.";

fn counts(delta: &Delta) -> String {
    format!("+{} ~{} −{}", delta.added.len(), delta.modified.len(), delta.deleted.len())
}

impl Run {
    /// Detect, the assets checks, Preview and Publish.
    pub(super) async fn assets_steps(&mut self) -> Result<(), RunFailure> {
        self.detect().await?;
        for step in [Step::Draft, Step::Write, Step::Build] {
            self.state.set_step(step, StepStatus::Skipped, Some(SKIPPED.to_owned()));
            self.journal.record(step, StepStatus::Skipped, Some(SKIPPED.to_owned()));
        }
        self.assets_verify()?;
        self.assets_preview().await?;
        if self.inputs.dry_run {
            return self.finish_dry_run().await;
        }
        self.assets_publish().await
    }

    fn assets_verify(&mut self) -> Result<(), RunFailure> {
        self.begin(Step::Verify, Phase::Verifying)?;
        if !self.inputs.project.assets_folder().is_some_and(|f| f.is_dir()) {
            return Err(RunFailure::new(
                "ASSETS_FOLDER_MISSING",
                "This project has no assets folder.",
                Some("Put banners, icons and screenshots in .wordpress-org, or set the assets folder in project settings.".to_owned()),
            ));
        }
        let facts = self.facts()?.clone();
        let readme = edit::read_text(&self.project_root(), README_FILE).ok();
        let assets = self.asset_names();
        let credentials = self.has_credentials();
        let results: Vec<_> = verify::run(&VerifyInput {
            facts: &facts,
            version: facts.header.version.as_deref().unwrap_or_default(),
            previous: None,
            server_tags: &[],
            readme_text: readme.as_deref(),
            main_file_text: "",
            files: &[],
            zip_entries: None,
            required_paths: &[],
            allow_phar: false,
            svn_version: self.inputs.svn.version.as_deref(),
            working_copy: &WorkingCopyState::NotCreated,
            has_credentials: credentials,
            git_dirty: None,
            current_wordpress: None,
            assets: assets.as_deref(),
            gitignored: &[],
        })
        .into_iter()
        .filter(|r| ASSETS_CHECKS.contains(&r.id.as_str()))
        .chain([verify::assets_check(&wporg_assets::inspect(
            self.inputs.project.assets_folder().as_deref(),
            None,
        ))])
        .collect();
        let blocked = verify::is_blocked(&results);
        self.state.checks = results;
        if blocked {
            return Err(RunFailure::new(
                "CHECKS_FAILED",
                "Blocking checks failed.",
                Some("Fix each failed check, then try again.".to_owned()),
            ));
        }
        self.done(Step::Verify, "Subversion and the account are ready".to_owned());
        Ok(())
    }

    async fn assets_preview(&mut self) -> Result<(), RunFailure> {
        self.begin(Step::Preview, Phase::Previewing)?;
        let url = self.inputs.project.svn_url.clone();
        let wc = self.inputs.paths.working_copy(&self.inputs.project.slug);
        self.svn().ensure_working_copy(&url, &wc, None).await?;
        let state = self.svn().working_copy_state(&wc, None).await?;
        let v15 = vec![verify::working_copy_check(&state)];
        let blocked = verify::is_blocked(&v15);
        self.replace_checks(v15);
        if blocked {
            return Err(RunFailure::new(
                "CHECKS_FAILED",
                "The working copy has conflicts or is out of date.",
                Some("Reset the working copy, then try again.".to_owned()),
            ));
        }

        let folder = self
            .inputs
            .project
            .assets_folder()
            .ok_or_else(|| RunFailure::new("RUN_STATE", "No assets folder.", None))?;
        self.previewed = true;
        let sources = svn::folder_files(&folder)?;
        let assets = self.svn().mirror(&sources, &wc.join("assets")).await?;
        if assets.is_empty() {
            return Err(RunFailure::new(
                "NOTHING_TO_RELEASE",
                "Nothing to release: assets/ on the server already matches your assets folder.",
                Some("Change a banner, icon or screenshot, then try again.".to_owned()),
            ));
        }
        let diffs = self.area_diffs(&wc, &["assets"]).await?;
        let summary = format!("assets {}", counts(&assets));
        self.state.preview = Some(SvnPreview {
            trunk: Delta::default(),
            assets,
            trunk_message: "Update assets".to_owned(),
            tag_message: String::new(),
            tag_url: String::new(),
            svn_url: url,
            account: self.account_name(),
            diffs,
        });
        self.done(Step::Preview, summary);
        Ok(())
    }

    async fn assets_publish(&mut self) -> Result<(), RunFailure> {
        let (message, _) =
            self.wait_for(Step::Publish, Phase::AwaitingPublish, publish_messages).await?;
        self.begin(Step::Publish, Phase::Publishing)?;
        let creds = credentials(&self.inputs)?;
        let wc = self.inputs.paths.working_copy(&self.inputs.project.slug);
        let revision = self.svn().commit_paths(&[wc.join("assets")], &message, &creds).await?;
        drop(creds);
        self.journal.revisions.assets = revision;
        self.journal.outcome = Some(Outcome::Complete);
        self.state.publish = Some(PublishResult {
            trunk_revision: None,
            tag_revision: None,
            assets_revision: revision,
            verification: None,
            plugin_url: self.inputs.project.plugin_page(),
            open_plugin_page: self.inputs.project.settings.post_publish_open_page,
            zip_path: None,
        });
        let summary =
            revision.map_or_else(|| "assets unchanged".to_owned(), |r| format!("assets r{r}"));
        self.done(Step::Publish, summary);
        self.state.phase = Phase::Verified;
        Ok(())
    }
}
