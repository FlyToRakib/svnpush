//! Steps 5 and 6: Build and Preview SVN.

use std::path::Path;

use crate::package::{self, Exclusions};
use crate::svn::{self, Delta};
use crate::vault;
use crate::verify::{self, FileRef, VerifyInput, WorkingCopyState};

use super::engine::Run;
use super::hook::run_pre_build;
use super::model::{FileDiff, Phase, Step, SvnPreview};

/// Total size of the per-file SVN diffs kept in the run state.
const DIFF_BUDGET_BYTES: usize = 2 * 1024 * 1024;
use super::RunFailure;

fn delta_counts(delta: &Delta) -> String {
    format!("+{} ~{} −{}", delta.added.len(), delta.modified.len(), delta.deleted.len())
}

impl Run {
    /// The username that will commit, when an account is set up.
    pub(super) fn account_name(&self) -> Option<String> {
        let host = vault::svn_host(&self.inputs.project.svn_url);
        vault::resolve_account(
            &self.inputs.accounts,
            &host,
            self.inputs.project.settings.svn_account.as_deref(),
        )
        .map(|a| a.username.clone())
    }

    /// Per-file diffs of the working-copy `areas`, keyed `<area>/<path>`,
    /// cut short once they reach the budget.
    pub(super) async fn area_diffs(
        &self,
        wc: &Path,
        areas: &[&str],
    ) -> Result<Vec<FileDiff>, RunFailure> {
        let mut diffs = Vec::new();
        let mut budget = DIFF_BUDGET_BYTES;
        for area in areas {
            if !wc.join(area).is_dir() {
                continue;
            }
            for (path, mut diff) in self.svn().diff_files(&wc.join(area)).await? {
                if diff.len() > budget {
                    let mut cut = budget;
                    while !diff.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    diff.truncate(cut);
                    diff.push_str("\n[diff cut short]\n");
                }
                budget = budget.saturating_sub(diff.len());
                diffs.push(FileDiff { path: format!("{area}/{path}"), diff });
            }
        }
        Ok(diffs)
    }

    /// Replaces rows in the check table by id.
    pub(super) fn replace_checks(&mut self, rows: Vec<verify::CheckResult>) {
        for row in rows {
            match self.state.checks.iter_mut().find(|c| c.id == row.id) {
                Some(existing) => *existing = row,
                None => self.state.checks.push(row),
            }
        }
    }

    pub(super) async fn build(&mut self) -> Result<(), RunFailure> {
        self.begin(Step::Build, Phase::Building)?;
        let facts = self.facts()?.clone();
        let version = self.journal.version.clone().unwrap_or_default();
        let slug = self.inputs.project.slug.clone();

        if let Some(command) =
            self.inputs.project.settings.pre_build_command.clone().filter(|c| !c.trim().is_empty())
        {
            let root = self.project_root();
            run_pre_build(&command, &root, self.observer.as_ref(), &self.cancel).await?;
        }
        let package_root = self.inputs.project.package_root();
        if !package_root.is_dir() {
            return Err(RunFailure::new(
                "PACKAGE_ROOT_MISSING",
                format!("The package root {} does not exist.", package_root.display()),
                Some(
                    "Check the package root and the pre-build command in project settings."
                        .to_owned(),
                ),
            ));
        }
        let builds = self.inputs.paths.builds();
        self.review_files(&package_root, &builds).await?;
        let listing = package::list(
            &package_root,
            &Exclusions::load(&package_root, std::slice::from_ref(&builds))?,
        )?;
        self.observer.info(&format!("Staging {} file(s).", listing.files.len()));
        let built = package::build(&listing, &builds, &slug, &version)?;
        self.package = Some(built.clone());

        let entries = package::zip_entry_names(Path::new(&built.zip_path))?;
        let files: Vec<FileRef<'_>> =
            built.files.iter().map(|f| FileRef { rel: &f.rel, size: f.size }).collect();
        let rows = verify::package_checks(&VerifyInput {
            facts: &facts,
            version: &version,
            previous: None,
            server_tags: &[],
            readme_text: None,
            main_file_text: "",
            files: &files,
            zip_entries: Some(&entries),
            required_paths: &self.inputs.project.settings.required_paths,
            allow_phar: self.inputs.project.settings.allow_phar,
            svn_version: None,
            working_copy: &WorkingCopyState::NotCreated,
            has_credentials: None,
            git_dirty: None,
            current_wordpress: None,
            assets: None,
            gitignored: &[],
        });
        let blocked = verify::is_blocked(&rows);
        self.replace_checks(rows);
        for path in &built.long_paths {
            self.state.notices.push(format!(
                "{path} is longer than 260 characters; some tools reject long paths."
            ));
        }
        self.state.package = Some(built.clone());
        if blocked {
            return Err(RunFailure::new(
                "CHECKS_FAILED",
                "The staged package failed a blocking check.",
                Some("Fix the package rules, then release again.".to_owned()),
            ));
        }
        if let Err(e) = package::prune_builds(&builds, &slug) {
            self.state.notices.push(format!("Could not prune old builds: {e}"));
        }
        self.done(
            Step::Build,
            format!(
                "{} file(s) · {} · SHA-256 {}",
                built.files.len(),
                human_size(built.total_size),
                &built.sha256[..12]
            ),
        );
        Ok(())
    }

    pub(super) async fn preview(&mut self) -> Result<(), RunFailure> {
        self.begin(Step::Preview, Phase::Previewing)?;
        let built = self
            .package
            .clone()
            .ok_or_else(|| RunFailure::new("RUN_STATE", "No package.", None))?;
        let version = self.journal.version.clone().unwrap_or_default();
        let slug = self.inputs.project.slug.clone();
        let url = self.inputs.project.svn_url.clone();
        let wc = self.inputs.paths.working_copy(&slug);

        self.svn().ensure_working_copy(&url, &wc, None).await?;
        let state = self.svn().working_copy_state(&wc, None).await?;
        let v15 = vec![verify::working_copy_check(&state)];
        let blocked = verify::is_blocked(&v15);
        self.replace_checks(v15);
        if blocked {
            return Err(RunFailure::new(
                "CHECKS_FAILED",
                "The working copy has conflicts or is out of date.",
                Some("Reset the working copy, then release again.".to_owned()),
            ));
        }

        self.previewed = true;
        let trunk = self
            .svn()
            .mirror(&svn::source_files(Path::new(&built.root), &built.files), &wc.join("trunk"))
            .await?;
        let assets = match self.inputs.project.assets_folder() {
            Some(folder) if folder.is_dir() => {
                let sources = svn::folder_files(&folder)?;
                self.svn().mirror(&sources, &wc.join("assets")).await?
            }
            _ => Delta::default(),
        };

        let account = self.account_name();
        let summary = format!("trunk {} · assets {}", delta_counts(&trunk), delta_counts(&assets));
        let diffs = self.area_diffs(&wc, &["trunk", "assets"]).await?;
        self.state.preview = Some(SvnPreview {
            trunk,
            assets,
            trunk_message: format!("Release {version}"),
            tag_message: format!("Tag {version}"),
            tag_url: format!("{}/tags/{version}", url.trim_end_matches('/')),
            svn_url: url,
            account,
            diffs,
        });
        self.done(Step::Preview, summary);
        Ok(())
    }
}

/// `1.4 MB`, `820 KB`, `12 B`.
pub(super) fn human_size(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let value = bytes as f64;
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", value / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", value / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_sizes() {
        assert_eq!(human_size(12), "12 B");
        assert_eq!(human_size(2048), "2 KB");
        assert_eq!(human_size(3 * 1024 * 1024 / 2), "1.5 MB");
    }
}
