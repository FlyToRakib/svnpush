//! Steps 1–4: Detect, Draft, Write, Verify.

use std::path::PathBuf;

use similar::TextDiff;

use crate::detect::git::Git;
use crate::detect::{self, DetectOptions, PluginFacts, header};
use crate::edit::{self, EditSet};
use crate::package::{self, Exclusions};
use crate::readme::{self, README_FILE};
use crate::svn::SvnError;
use crate::vault;
use crate::verify::{self, CheckResult, CheckStatus, FileRef, VerifyInput, WorkingCopyState};
use crate::version::{self, Version};
use crate::wporg_assets;

use crate::project::AiChoice;

use super::RunFailure;
use super::changes::{self, ChangeSet, ChangeSource};
use super::draft::{prefill, suggested_version};
use super::engine::Run;
use super::material::{self, Filter, Material};
use super::model::{DraftContext, ErrorView, FileDiff, Phase, Step};
use super::snapshot::Snapshot;

/// Unified diff of one file, three lines of context.
pub(super) fn unified_diff(path: &str, before: &str, after: &str) -> String {
    TextDiff::from_lines(before, after)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

impl Run {
    pub(super) fn project_root(&self) -> PathBuf {
        PathBuf::from(&self.inputs.project.path)
    }

    pub(super) fn detect_facts(&self) -> Result<PluginFacts, RunFailure> {
        let settings = &self.inputs.project.settings;
        Ok(detect::detect(
            &self.project_root(),
            DetectOptions {
                svn_url: &self.inputs.project.svn_url,
                main_file: settings.main_file.as_deref(),
                version_locations: &settings.version_locations,
            },
        )?)
    }

    pub(super) fn facts(&self) -> Result<&PluginFacts, RunFailure> {
        self.state
            .facts
            .as_ref()
            .ok_or_else(|| RunFailure::new("RUN_STATE", "Detect has not run.", None))
    }

    pub(super) async fn detect(&mut self) -> Result<(), RunFailure> {
        self.begin(Step::Detect, Phase::Detecting)?;
        if !self.inputs.svn.ok {
            return Err(RunFailure::new(
                "TOOLS_SVN_UNAVAILABLE",
                self.inputs.svn.message.clone(),
                self.inputs.svn.fix.clone(),
            ));
        }
        let facts = self.detect_facts()?;
        self.journal.main_file = Some(facts.main_file.clone());

        if let Some(git_bin) = self.inputs.git.clone() {
            let root = self.project_root();
            let git = Git::new(&git_bin, &root, self.observer.as_ref(), &self.cancel);
            match git.facts().await {
                Ok(found) => self.state.git = found,
                Err(crate::tools::ProcessError::Cancelled) => return Err(RunFailure::cancelled()),
                Err(e) => self.state.notices.push(format!("Git facts are unavailable: {e}")),
            }
        }

        let slug = self.inputs.project.slug.clone();
        let trunk_version = {
            let trunk = self.inputs.paths.working_copy(&slug).join("trunk");
            edit::read_text(&trunk, &facts.main_file)
                .ok()
                .and_then(|text| header::parse(&text).version)
                .filter(|v| Version::parse(v).is_ok())
        };
        let url = self.inputs.project.svn_url.clone();
        let tags = self.svn().list_tags(&url, None).await;
        match tags {
            Ok(tags) => self.state.server_tags = tags,
            Err(SvnError::Network { .. }) => self.state.notices.push(
                "Could not list tags on the server. The previous release comes from the local working copy only."
                    .to_owned(),
            ),
            Err(e) => return Err(e.into()),
        }
        let previous = trunk_version
            .iter()
            .map(String::as_str)
            .chain(self.state.server_tags.iter().map(String::as_str))
            .filter_map(|v| Version::parse(v).ok())
            .max();

        let summary = format!(
            "{} {} · {} · {} tag(s) on the server",
            facts.name,
            facts.header.version.as_deref().unwrap_or("(no version)"),
            facts.slug,
            self.state.server_tags.len()
        );
        self.state.previous = previous.map(|p| p.to_string());
        self.state.facts = Some(facts);
        self.done(Step::Detect, summary);
        Ok(())
    }

    pub(super) async fn draft(&mut self) -> Result<(), RunFailure> {
        self.begin(Step::Draft, Phase::Drafting)?;
        let facts = self.facts()?.clone();
        let previous = self.state.previous.clone();
        let commits = self.state.git.as_ref().map(|g| g.commits.clone()).unwrap_or_default();

        let mut change_set = None;
        if let (Some(git_bin), Some(git_facts)) = (self.inputs.git.clone(), self.state.git.clone())
        {
            let root = self.project_root();
            let git = Git::new(&git_bin, &root, self.observer.as_ref(), &self.cancel);
            change_set = changes::from_git(&git, &git_facts)
                .await
                .map_err(|e| RunFailure::new("GIT_FAILED", e.to_string(), None))?;
        }
        let change_set = if let Some(set) = change_set {
            set
        } else {
            let wc = self.inputs.paths.working_copy(&self.inputs.project.slug);
            let url = self.inputs.project.svn_url.clone();
            self.svn().ensure_working_copy(&url, &wc, None).await?;
            changes::from_trunk(&self.inputs.project.package_root(), &wc.join("trunk"), commits)?
        };
        if change_set.is_empty() {
            return Err(RunFailure::new(
                "NOTHING_TO_RELEASE",
                "Nothing to release: the plugin matches the previous release.",
                Some("Make your changes, then release again.".to_owned()),
            ));
        }

        let material = if self.inputs.project.settings.ai_provider == AiChoice::Off {
            Material::default()
        } else {
            self.material(&change_set).await?
        };
        let version = suggested_version(&facts, previous.as_deref());
        let context = DraftContext {
            previous,
            prefill: prefill(&facts, &version, &change_set),
            suggested_version: version.clone(),
            changes: change_set,
        };
        let files = context.changes.files.len();
        self.state.draft_context = Some(context);

        let draft = self.decide_draft(&facts, &material, &version).await?;
        self.journal.version = Some(draft.version.clone());
        let summary = format!("Version {} approved · {files} changed file(s)", draft.version);
        self.state.draft = Some(draft);
        self.done(Step::Draft, summary);
        Ok(())
    }

    pub(super) fn write(&mut self) -> Result<(), RunFailure> {
        self.begin(Step::Write, Phase::Writing)?;
        let facts = self.facts()?.clone();
        let draft = self
            .state
            .draft
            .clone()
            .ok_or_else(|| RunFailure::new("RUN_STATE", "No draft.", None))?;
        let root = self.project_root();
        let settings = self.inputs.project.settings.clone();

        let mut edits = EditSet::new(&root);
        version::write_version(
            &mut edits,
            &facts.main_file,
            &settings.version_locations,
            &draft.version,
        )?;
        if root.join(README_FILE).is_file() {
            edits.modify::<RunFailure>(README_FILE, |text| {
                let parsed = readme::parse(text);
                let mut next = if parsed.header("Stable tag").is_some() {
                    readme::set_header(text, "Stable tag", &draft.version)?
                } else {
                    text.to_owned()
                };
                next = readme::upsert_changelog_entry(
                    &next,
                    &draft.version,
                    &draft.changelog_markdown,
                );
                Ok(readme::upsert_upgrade_notice(&next, &draft.version, &draft.upgrade_notice))
            })?;
        }
        let changed = edits.changed();
        let snapshot_dir = self
            .inputs
            .paths
            .runs(&self.inputs.project.slug)
            .join(&self.journal.id)
            .join("snapshot");
        self.snapshot = Some(Snapshot::take(&root, &snapshot_dir, &changed)?);
        self.journal.snapshot = Some(snapshot_dir.display().to_string());
        self.save_journal();
        edits.write()?;

        let diffs: Vec<FileDiff> = changed
            .iter()
            .map(|e| FileDiff {
                path: e.path.clone(),
                diff: unified_diff(&e.path, &e.before, &e.after),
            })
            .collect();
        self.journal.diffs.clone_from(&diffs);
        self.state.diffs = diffs;
        self.state.facts = Some(self.detect_facts()?);
        self.done(Step::Write, format!("{} file(s) written", changed.len()));
        Ok(())
    }

    /// File names in the assets folder, when the project has one.
    pub(super) fn asset_names(&self) -> Option<Vec<String>> {
        self.inputs.project.assets_folder().map(|dir| {
            std::fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    pub(super) fn has_credentials(&self) -> Option<bool> {
        if self.inputs.dry_run {
            return None;
        }
        let host = vault::svn_host(&self.inputs.project.svn_url);
        let chosen = self.inputs.project.settings.svn_account.as_deref();
        Some(
            vault::resolve_account(&self.inputs.accounts, &host, chosen)
                .is_some_and(|a| matches!(self.inputs.vault.get(&a.key()), Ok(Some(_)))),
        )
    }

    /// What the AI sees of the changes.
    async fn material(&self, changes: &ChangeSet) -> Result<Material, RunFailure> {
        let filter = Filter::new(&self.inputs.project.settings.ai_exclude_patterns);
        if changes.source == ChangeSource::Git
            && let Some(git_bin) = self.inputs.git.clone()
        {
            let root = self.project_root();
            let git = Git::new(&git_bin, &root, self.observer.as_ref(), &self.cancel);
            return material::from_git(&git, &root, changes, &filter)
                .await
                .map_err(|e| RunFailure::new("GIT_FAILED", e.to_string(), None));
        }
        let trunk = self.inputs.paths.working_copy(&self.inputs.project.slug).join("trunk");
        Ok(material::from_trunk(&self.inputs.project.package_root(), &trunk, changes, &filter))
    }

    /// Step 4: the checks, then (when a blocking check fails) the AI's
    /// explanation and suggested readme fixes, verifying again after fixes.
    pub(super) async fn verify(&mut self) -> Result<(), RunFailure> {
        loop {
            self.state.explanation = None;
            if self.check_once().await? {
                return Ok(());
            }
            if !self.review_failures().await? {
                return Err(RunFailure::new(
                    "CHECKS_FAILED",
                    "Blocking checks failed.",
                    Some("Fix each failed check, then release again.".to_owned()),
                ));
            }
        }
    }

    /// Runs every check once; `true` when nothing blocks.
    async fn check_once(&mut self) -> Result<bool, RunFailure> {
        self.begin(Step::Verify, Phase::Verifying)?;
        let facts = self.facts()?.clone();
        let draft = self
            .state
            .draft
            .clone()
            .ok_or_else(|| RunFailure::new("RUN_STATE", "No draft.", None))?;
        let root = self.project_root();
        let package_root = self.inputs.project.package_root();
        let package_ready = package_root.is_dir();

        let listing = if package_ready {
            Some(package::list(
                &package_root,
                &Exclusions::load(&package_root, &[self.inputs.paths.builds()])?,
            )?)
        } else {
            None
        };
        let files: Vec<FileRef<'_>> = listing
            .iter()
            .flat_map(|l| l.files.iter().map(|f| FileRef { rel: &f.rel, size: f.size }))
            .collect();
        let rels: Vec<String> = files.iter().map(|f| f.rel.to_owned()).collect();
        let gitignored = package::gitignored(&package_root, &rels);
        let readme_text = edit::read_text(&root, README_FILE).ok();
        let main_text = edit::read_text(&root, &facts.main_file)?;
        let wc = self.inputs.paths.working_copy(&self.inputs.project.slug);
        let (working_copy, wc_error) = match self.svn().working_copy_state(&wc, None).await {
            Ok(state) => (state, None),
            Err(SvnError::Cancelled) => return Err(RunFailure::cancelled()),
            Err(e) => (WorkingCopyState::NotCreated, Some(ErrorView::from_coded(&e))),
        };
        let assets = self.asset_names();
        let credentials = self.has_credentials();

        let mut results = verify::run(&VerifyInput {
            facts: &facts,
            version: &draft.version,
            previous: self.state.previous.as_deref(),
            server_tags: &self.state.server_tags,
            readme_text: readme_text.as_deref(),
            main_file_text: &main_text,
            files: &files,
            zip_entries: None,
            required_paths: &self.inputs.project.settings.required_paths,
            allow_phar: self.inputs.project.settings.allow_phar,
            svn_version: self.inputs.svn.version.as_deref(),
            working_copy: &working_copy,
            has_credentials: credentials,
            git_dirty: self.state.git.as_ref().map(|g| g.dirty.as_slice()),
            current_wordpress: self.inputs.current_wordpress.as_deref(),
            assets: assets.as_deref(),
            gitignored: &gitignored,
        });
        adjust(&mut results, package_ready, wc_error.as_ref());
        let asset_folder = self.inputs.project.assets_folder();
        results.push(verify::assets_check(&wporg_assets::inspect(asset_folder.as_deref(), None)));
        let blocked = verify::is_blocked(&results);
        let failed = results.iter().filter(|r| r.status == CheckStatus::Fail).count();
        self.state.checks = results;
        if blocked {
            return Ok(false);
        }
        self.done(Step::Verify, format!("All blocking checks passed · {failed} warning(s)"));
        Ok(true)
    }
}

/// Step 4 context the pure checks cannot see: a package root a pre-build
/// command has yet to create, and a server that could not be asked.
fn adjust(results: &mut [CheckResult], package_ready: bool, wc_error: Option<&ErrorView>) {
    for result in results.iter_mut() {
        if !package_ready && matches!(result.id.as_str(), "V10" | "V11" | "V12") {
            result.status = CheckStatus::Skip;
            "Runs after the pre-build command creates the package root."
                .clone_into(&mut result.message);
            result.fix = None;
            result.paths.clear();
        }
        if result.id == "V15"
            && let Some(error) = wc_error
        {
            result.status = CheckStatus::Fail;
            result.message = format!("Could not check the working copy: {}", error.message);
            result.fix.clone_from(&error.fix);
        }
    }
}
