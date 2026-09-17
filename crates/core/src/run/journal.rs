//! The run journal: one JSON file per run, written after every step, so a
//! crash leaves a record to resume from (plan §12.5).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::clock;
use crate::project::{self, AppPaths, ConfigError};
use crate::svn::TagVerification;

use super::model::{FileDiff, Step, StepStatus};

/// Snapshots of successful publishes are kept this long (plan §12.6).
pub const SNAPSHOT_RETENTION_SECONDS: u64 = 7 * 24 * 60 * 60;

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "step")]
#[ts(export)]
pub enum Outcome {
    /// Published and verified.
    Complete,
    /// Published; verification did not confirm the tag.
    PublishedUnverified,
    /// A dry run finished.
    DryRun,
    /// A step failed.
    Failed(Step),
    /// Cancelled by the developer, or discarded after an interruption.
    Cancelled,
}

impl Outcome {
    /// The short label stored on the project.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Complete => "Complete",
            Self::PublishedUnverified => "PublishedUnverified",
            Self::DryRun => "DryRun",
            Self::Failed(_) => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// One step's record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StepRecord {
    /// Which step.
    pub step: Step,
    /// How it ended.
    pub status: StepStatus,
    /// ISO 8601 UTC time.
    pub at: String,
    /// One line.
    pub summary: Option<String>,
}

/// Revisions created on the server.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Revisions {
    /// The trunk commit.
    #[ts(type = "number | null")]
    pub trunk: Option<u64>,
    /// The tag copy.
    #[ts(type = "number | null")]
    pub tag: Option<u64>,
    /// The assets commit of an assets-only release.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub assets: Option<u64>,
}

/// A run's journal (plan §13 `RunJournal`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunJournal {
    /// Run id, also the file name.
    pub id: String,
    /// Plugin slug.
    pub slug: String,
    /// Project folder.
    pub project_path: String,
    /// The version released, once approved.
    pub version: Option<String>,
    /// Main plugin file, for tag verification on resume.
    pub main_file: Option<String>,
    /// Whether this was a dry run.
    pub dry_run: bool,
    /// ISO 8601 UTC start time.
    pub started: String,
    /// ISO 8601 UTC end time.
    pub finished: Option<String>,
    /// Steps, in the order they ended.
    pub steps: Vec<StepRecord>,
    /// Server revisions.
    pub revisions: Revisions,
    /// How it ended; `None` while running or after a crash.
    pub outcome: Option<Outcome>,
    /// The diffs Step 3 wrote.
    pub diffs: Vec<FileDiff>,
    /// Tag message used, for resume.
    pub tag_message: Option<String>,
    /// Verification result.
    pub verification: Option<TagVerification>,
    /// Snapshot folder, while it exists.
    pub snapshot: Option<String>,
    /// Whether the developer discarded this run after it stopped.
    #[serde(default)]
    pub discarded: bool,
    /// Whether the run synced and committed only `assets/`.
    #[serde(default)]
    pub assets_only: bool,
}

impl RunJournal {
    /// A journal for a run starting now.
    pub fn start(id: &str, slug: &str, project_path: &str, dry_run: bool) -> Self {
        Self {
            id: id.to_owned(),
            slug: slug.to_owned(),
            project_path: project_path.to_owned(),
            version: None,
            main_file: None,
            dry_run,
            started: clock::iso8601(clock::now()),
            finished: None,
            steps: Vec::new(),
            revisions: Revisions::default(),
            outcome: None,
            diffs: Vec::new(),
            tag_message: None,
            verification: None,
            snapshot: None,
            discarded: false,
            assets_only: false,
        }
    }

    /// Whether the run stopped without an outcome (the app closed mid-run).
    pub fn is_interrupted(&self) -> bool {
        self.outcome.is_none()
    }

    /// Whether trunk was committed but no tag was created: Resume creates only the tag.
    pub fn needs_tag(&self) -> bool {
        self.revisions.trunk.is_some()
            && self.revisions.tag.is_none()
            && !self.dry_run
            && !self.discarded
    }

    /// The last step that finished.
    pub fn last_step(&self) -> Option<Step> {
        self.steps.last().map(|s| s.step)
    }

    /// Records a finished step.
    pub fn record(&mut self, step: Step, status: StepStatus, summary: Option<String>) {
        self.steps.push(StepRecord { step, status, at: clock::iso8601(clock::now()), summary });
    }

    /// The file this journal is saved to.
    pub fn file(&self, paths: &AppPaths) -> PathBuf {
        paths.runs(&self.slug).join(format!("{}.json", self.id))
    }

    /// Saves the journal.
    pub fn save(&self, paths: &AppPaths) -> Result<(), ConfigError> {
        project::write_json(&self.file(paths), self)
    }
}

/// Every journal for a plugin, newest first.
pub fn list(paths: &AppPaths, slug: &str) -> Result<Vec<RunJournal>, ConfigError> {
    let dir = paths.runs(slug);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(ConfigError::Io {
                action: "read",
                path: dir.display().to_string(),
                source,
            });
        }
    };
    let mut journals = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json")
            && let Some(journal) = project::read_json::<RunJournal>(&path)?
        {
            journals.push(journal);
        }
    }
    journals.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(journals)
}

/// Deletes snapshot folders of successful publishes older than the retention period.
pub fn prune_snapshots(paths: &AppPaths, slug: &str) -> Result<(), ConfigError> {
    for mut journal in list(paths, slug)? {
        let Some(snapshot) = journal.snapshot.clone() else { continue };
        let published =
            matches!(journal.outcome, Some(Outcome::Complete | Outcome::PublishedUnverified));
        let age = std::fs::metadata(Path::new(&snapshot))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map_or(0, |d| d.as_secs());
        if published && age >= SNAPSHOT_RETENTION_SECONDS {
            let dir = Path::new(&snapshot);
            if dir.exists() {
                std::fs::remove_dir_all(dir).map_err(|source| ConfigError::Io {
                    action: "remove",
                    path: snapshot.clone(),
                    source,
                })?;
            }
            journal.snapshot = None;
            journal.save(paths)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journals_list_newest_first_and_detect_interruption() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        let mut old = RunJournal::start("20260101-000000", "demo", "/p", false);
        old.outcome = Some(Outcome::Complete);
        old.save(&paths).unwrap();
        let mut new = RunJournal::start("20260917-120000", "demo", "/p", false);
        new.record(Step::Publish, StepStatus::Running, None);
        new.revisions.trunk = Some(12);
        new.save(&paths).unwrap();

        let all = list(&paths, "demo").unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "20260917-120000");
        assert!(all[0].is_interrupted());
        assert!(all[0].needs_tag());
        assert_eq!(all[0].last_step(), Some(Step::Publish));
        assert!(!all[1].is_interrupted());
        assert!(list(&paths, "other").unwrap().is_empty());
    }

    #[test]
    fn outcome_serialises_with_the_failed_step() {
        let text = serde_json::to_string(&Outcome::Failed(Step::Verify)).unwrap();
        assert_eq!(text, r#"{"kind":"Failed","step":"Verify"}"#);
    }
}
