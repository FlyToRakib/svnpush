//! Step 5's safety stop: before anything is staged, the developer sees what
//! will be released and what is left out, and confirms the `.distignore`
//! (SVNpush proposes one when the plugin has none). It asks only when it
//! matters: no `.distignore`, the first release, or new top-level files and
//! folders that were not in the last release.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::detect::DISTIGNORE_FILE;
use crate::package::{self, Exclusions, PackageError};
use crate::svn::SvnError;

use super::RunFailure;
use super::engine::Run;
use super::model::{Decision, Phase, Step};

/// Why SVNpush asks the developer to check the files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum ReviewReason {
    /// The plugin has no `.distignore`; SVNpush proposes one.
    NoDistignore,
    /// Trunk on the server is empty: everything is new.
    FirstRelease,
    /// Top-level files or folders that are not in trunk yet.
    NewItems,
}

/// One file that will be released.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ReviewFile {
    /// Path relative to the package root.
    pub path: String,
    /// Size in bytes.
    #[ts(type = "number")]
    pub size: u64,
}

/// What a set of rules releases and leaves out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FilePreview {
    /// Files that will be released.
    pub files: Vec<ReviewFile>,
    /// Left-out paths; a left-out folder appears once, ending with `/`.
    pub excluded: Vec<String>,
    /// How many left-out paths there are in total.
    pub excluded_total: u32,
}

/// The file check shown in Step 5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FileReview {
    /// Why it is shown.
    pub reasons: Vec<ReviewReason>,
    /// Whether the rules can be edited here. A package root that a pre-build
    /// command creates is controlled by that command instead.
    pub editable: bool,
    /// Whether `.distignore` already exists.
    pub distignore_exists: bool,
    /// The current `.distignore`, or the one SVNpush proposes.
    pub distignore: String,
    /// What those rules release and leave out.
    pub preview: FilePreview,
    /// Top-level files and folders that are not in trunk yet (folders end with `/`).
    pub new_items: Vec<String>,
}

/// What `distignore` would release from `root`, ignoring `skip` folders.
pub fn preview(
    root: &Path,
    distignore: &str,
    skip: &[PathBuf],
) -> Result<FilePreview, PackageError> {
    let split = package::split(root, &Exclusions::from_text(root, distignore, skip)?)?;
    Ok(FilePreview {
        files: split
            .listing
            .files
            .into_iter()
            .map(|f| ReviewFile { path: f.rel, size: f.size })
            .collect(),
        excluded: split.excluded,
        excluded_total: u32::try_from(split.excluded_total).unwrap_or(u32::MAX),
    })
}

/// The top-level entries the files live in: `plugin.php`, `includes/`.
fn top_level(files: &[ReviewFile]) -> BTreeSet<String> {
    files
        .iter()
        .map(|f| match f.path.split_once('/') {
            Some((folder, _)) => format!("{folder}/"),
            None => f.path.clone(),
        })
        .collect()
}

/// Normalises saved text: `\n` line ends and one final newline.
fn normalise(text: &str) -> String {
    let mut out = text.replace("\r\n", "\n").trim_end().to_owned();
    out.push('\n');
    out
}

impl Run {
    /// Shows the file check when it matters and waits for the developer.
    /// Confirming saves an edited or proposed `.distignore`.
    pub(super) async fn review_files(
        &mut self,
        root: &Path,
        builds: &Path,
    ) -> Result<(), RunFailure> {
        let settings = &self.inputs.project.settings;
        let editable = settings.pre_build_command.as_deref().is_none_or(|c| c.trim().is_empty())
            || root == self.project_root();
        let path = root.join(DISTIGNORE_FILE);
        let existing = std::fs::read_to_string(&path).ok();
        let distignore = existing.clone().unwrap_or_else(|| package::suggested_distignore(root));
        let skip = [builds.to_path_buf()];
        let listing = preview(root, &distignore, &skip)?;

        let mut reasons = Vec::new();
        if existing.is_none() && editable {
            reasons.push(ReviewReason::NoDistignore);
        }
        let trunk_url = format!("{}/trunk", self.inputs.project.svn_url.trim_end_matches('/'));
        let mut new_items = Vec::new();
        match self.svn().list(&trunk_url, None).await {
            Ok(entries) if entries.is_empty() => reasons.push(ReviewReason::FirstRelease),
            Ok(entries) => {
                let on_server: BTreeSet<String> = entries.into_iter().collect();
                new_items = top_level(&listing.files).difference(&on_server).cloned().collect();
                if !new_items.is_empty() {
                    reasons.push(ReviewReason::NewItems);
                }
            }
            Err(SvnError::Cancelled) => return Err(RunFailure::cancelled()),
            Err(e) => self
                .state
                .notices
                .push(format!("Could not compare the files with trunk on the server: {e}")),
        }
        if reasons.is_empty() {
            return Ok(());
        }

        self.state.file_review = Some(FileReview {
            reasons,
            editable,
            distignore_exists: existing.is_some(),
            distignore: distignore.clone(),
            preview: listing,
            new_items,
        });
        let answer = self
            .wait_for(Step::Build, Phase::AwaitingFileReview, |d| match d {
                Decision::ConfirmFiles { distignore } => Some(distignore),
                _ => None,
            })
            .await?;

        if editable
            && let Some(text) = answer.map(|t| normalise(&t))
            && existing.as_deref() != Some(text.as_str())
        {
            Exclusions::from_text(root, &text, &skip)?;
            std::fs::write(&path, &text).map_err(|e| RunFailure::io("write", &path, &e))?;
            self.state.notices.push(format!(
                "Saved {} in {}. Commit it with your plugin so every release uses the same rules.",
                DISTIGNORE_FILE,
                root.display()
            ));
        }
        self.begin(Step::Build, Phase::Building)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_uses_the_given_text_and_top_level_names_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("includes")).unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("plugin.php"), "<?php").unwrap();
        std::fs::write(root.join("includes/a.php"), "a").unwrap();
        std::fs::write(root.join("docs/guide.md"), "g").unwrap();

        let all = preview(root, "", &[]).unwrap();
        assert_eq!(all.files.len(), 3);
        let trimmed = preview(root, "/docs\n", &[]).unwrap();
        assert_eq!(trimmed.excluded, ["docs/"]);
        assert_eq!(
            top_level(&trimmed.files).into_iter().collect::<Vec<_>>(),
            ["includes/", "plugin.php"]
        );
    }

    #[test]
    fn saved_text_is_normalised() {
        assert_eq!(normalise("/docs\r\n.*\r\n\r\n"), "/docs\n.*\n");
    }
}
