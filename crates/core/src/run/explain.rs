//! Step 4 after a blocking failure: the AI explains the failed checks and
//! suggests readme edits the developer can apply with one click (plan §5.4).
//! Code is never edited: only `readme.txt`, and only text that occurs exactly once.

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::ai::client::ClientError;
use crate::ai::prompts::{self, FailedCheck};
use crate::ai::schemas::{self, ExplainAnswer, SuggestedFix};
use crate::ai::task::{Answer, TaskError};
use crate::edit::{self, EditSet};
use crate::readme::README_FILE;
use crate::verify::CheckStatus;

use super::RunFailure;
use super::ai_view::{AiStatus, AiTask, Explanation, FixView};
use super::assist::{Events, Job, Resolved};
use super::engine::Run;
use super::material::Filter;
use super::model::{Decision, ErrorView, FileDiff, Phase, RunState, Step, StepStatus};
use super::steps::unified_diff;

/// Characters of the main plugin file sent with an explanation request.
const MAIN_FILE_EXCERPT_CHARS: usize = 4_000;

fn explain_task(state: &mut RunState) -> Option<&mut AiTask> {
    state.explanation.as_mut().map(|e| &mut e.task)
}

/// Checks a suggested fix against the current readme.
pub(super) fn fix_view(fix: SuggestedFix, readme: Option<&str>) -> FixView {
    let mut view = FixView {
        check_id: fix.check_id,
        path: fix.path,
        original: fix.original,
        replacement: fix.replacement,
        diff: String::new(),
        problem: None,
    };
    let problem = if view.path != README_FILE {
        Some("Only readme.txt edits can be applied. Make this change yourself.")
    } else if view.original.is_empty() || view.original == view.replacement {
        Some("The suggestion does not change anything.")
    } else {
        match readme {
            None => Some("There is no readme.txt to edit."),
            Some(text) => match text.matches(view.original.as_str()).count() {
                1 => {
                    let after = text.replacen(view.original.as_str(), &view.replacement, 1);
                    view.diff = unified_diff(README_FILE, text, &after);
                    None
                }
                0 => Some("The text to replace is not in readme.txt."),
                _ => Some("The text to replace appears more than once in readme.txt."),
            },
        }
    };
    view.problem = problem.map(str::to_owned);
    view
}

async fn write_explanation(
    job: Job,
    prompt: prompts::Prompt,
    cancel: CancellationToken,
    events: Events,
) -> Result<Answer<ExplainAnswer>, TaskError> {
    tokio::select! {
        result = job.ask(&prompt, &schemas::EXPLAIN_FAILURES, "explain_failures", &cancel, &events) => result,
        never = job.watch_fleet(&events) => match never {},
    }
}

impl Run {
    /// After blocking checks fail: explain them when AI is available, and wait
    /// for fixes or Stop. `true` means fixes were applied and Verify runs again.
    pub(super) async fn review_failures(&mut self) -> Result<bool, RunFailure> {
        let mut next = match self.resolve_ai(None) {
            Resolved::Off | Resolved::NoProvider(_) => return Ok(false),
            Resolved::Ready(_) => Some(Decision::Generate { provider_id: None }),
        };
        let task = AiTask::new(AiStatus::Working, self.provider_choices());
        self.state.explanation = Some(Explanation { task, text: None, fixes: Vec::new() });
        loop {
            let decision = match next.take() {
                Some(decision) => decision,
                None => self.wait_for(Step::Verify, Phase::AwaitingFixes, Some).await?,
            };
            match decision {
                Decision::Stop => return Ok(false),
                Decision::ApplyFixes { fixes } => {
                    if self.apply_fixes(&fixes)? {
                        return Ok(true);
                    }
                }
                Decision::AcceptPrivacy { provider_id } => {
                    self.accept_privacy(&provider_id);
                    next = Some(Decision::Generate { provider_id: Some(provider_id) });
                }
                Decision::Generate { provider_id } => match self.choose_ai(provider_id) {
                    Resolved::Off => {}
                    Resolved::NoProvider(error) => {
                        if let Some(task) = explain_task(&mut self.state) {
                            task.status = AiStatus::NoProvider;
                            task.error = Some(error);
                        }
                    }
                    Resolved::Ready(record) if !self.privacy_seen(&record.id) => {
                        if let Some(task) = explain_task(&mut self.state) {
                            task.status = AiStatus::NeedsConsent;
                            task.provider = Some((&record).into());
                            task.privacy = Some(Self::privacy_notice(&record));
                        }
                    }
                    Resolved::Ready(record) => next = self.generate_explanation(record).await?,
                },
                Decision::Manual => {
                    if let Some(task) = explain_task(&mut self.state) {
                        task.status = AiStatus::Manual;
                        task.privacy = None;
                    }
                }
                Decision::Approve { .. }
                | Decision::Publish { .. }
                | Decision::ConfirmFiles { .. } => {}
            }
        }
    }

    async fn generate_explanation(
        &mut self,
        record: crate::ai::records::ProviderRecord,
    ) -> Result<Option<Decision>, RunFailure> {
        self.state.set_step(Step::Verify, StepStatus::Running, None);
        if let Some(explanation) = self.state.explanation.as_mut() {
            explanation.task.start((&record).into());
            explanation.text = None;
            explanation.fixes.clear();
        }
        self.emit();

        let root = self.project_root();
        let filter = Filter::new(&self.inputs.project.settings.ai_exclude_patterns);
        let readme = edit::read_text(&root, README_FILE).ok();
        let mut excerpts = Vec::new();
        if let Some(text) = readme.as_ref().filter(|_| !filter.excludes(README_FILE)) {
            excerpts.push((README_FILE.to_owned(), text.clone()));
        }
        let main_file: Option<String> = self.state.facts.as_ref().map(|f| f.main_file.clone());
        if let Some(main) = main_file.filter(|m| !filter.excludes(m.as_str()))
            && let Ok(text) = edit::read_text(&root, &main)
        {
            let excerpt = prompts::truncate_chars(&text, MAIN_FILE_EXCERPT_CHARS);
            excerpts.push((main, excerpt));
        }
        let failed: Vec<FailedCheck<'_>> = self
            .state
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .map(|c| FailedCheck {
                id: &c.id,
                title: &c.title,
                message: &c.message,
                fix: c.fix.as_deref(),
            })
            .collect();
        let prompt = prompts::explain_failures(&failed, &excerpts);
        let version = self.state.draft.as_ref().map(|d| d.version.clone()).unwrap_or_default();

        let job = self.job(record, &version);
        let (sender, receiver) = mpsc::unbounded_channel();
        let stop = self.cancel.child_token();
        let work = write_explanation(job, prompt, stop.clone(), sender);
        let (result, pending) = self.pump(work, receiver, &stop, explain_task).await?;

        let Some(explanation) = self.state.explanation.as_mut() else { return Ok(pending) };
        explanation.task.job_status = None;
        explanation.task.queue_position = None;
        match result {
            Ok(answer) => {
                explanation.task.status = AiStatus::Done;
                explanation.text = Some(answer.value.explanation);
                explanation.fixes = answer
                    .value
                    .fixes
                    .into_iter()
                    .map(|fix| fix_view(fix, readme.as_deref()))
                    .collect();
            }
            Err(TaskError::Client(ClientError::Cancelled)) => {
                explanation.task.status = AiStatus::Manual;
            }
            Err(error) => {
                explanation.task.status = AiStatus::Failed;
                explanation.task.error = Some(ErrorView::from_coded(&error));
                explanation.task.raw_text = match &error {
                    TaskError::Invalid { raw_text, .. } | TaskError::Truncated { raw_text, .. } => {
                        Some(raw_text.clone())
                    }
                    TaskError::Client(_) | TaskError::Refused { .. } => None,
                };
            }
        }
        self.emit();
        Ok(pending)
    }

    /// Applies the chosen readme fixes. The readme is already in the Step 3
    /// snapshot (Step 3 writes the changelog), so rollback restores it.
    fn apply_fixes(&mut self, indexes: &[u32]) -> Result<bool, RunFailure> {
        let Some(explanation) = self.state.explanation.clone() else { return Ok(false) };
        let root = self.project_root();
        let mut edits = EditSet::new(&root);
        let mut applied = 0;
        for fix in indexes.iter().filter_map(|&i| explanation.fixes.get(usize::try_from(i).ok()?)) {
            if fix.problem.is_some() {
                continue;
            }
            edits.modify::<RunFailure>(README_FILE, |text| {
                if text.matches(fix.original.as_str()).count() == 1 {
                    applied += 1;
                    Ok(text.replacen(fix.original.as_str(), &fix.replacement, 1))
                } else {
                    Ok(text.to_owned())
                }
            })?;
        }
        let changed = edits.changed();
        if changed.is_empty() {
            self.state.notices.push("None of the chosen fixes could be applied; the readme changed since they were suggested.".to_owned());
            self.emit();
            return Ok(false);
        }
        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.include(&changed)?;
        }
        edits.write()?;
        for edit in &changed {
            let diff = FileDiff {
                path: edit.path.clone(),
                diff: unified_diff(&edit.path, &edit.before, &edit.after),
            };
            self.journal.diffs.push(diff.clone());
            self.state.diffs.push(diff);
        }
        self.save_journal();
        self.state.notices.push(format!("Applied {applied} suggested fix(es) to readme.txt."));
        self.state.facts = Some(self.detect_facts()?);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(path: &str, original: &str, replacement: &str) -> SuggestedFix {
        SuggestedFix {
            check_id: "V06".into(),
            path: path.into(),
            original: original.into(),
            replacement: replacement.into(),
        }
    }

    #[test]
    fn only_unique_readme_text_can_be_replaced() {
        let readme = "Stable tag: trunk\nTested up to: 6.0\n";
        let ok =
            fix_view(fix("readme.txt", "Stable tag: trunk", "Stable tag: 1.2.0"), Some(readme));
        assert_eq!(ok.problem, None);
        assert!(ok.diff.contains("-Stable tag: trunk\n+Stable tag: 1.2.0"));

        let code = fix_view(fix("plugin.php", "a", "b"), Some(readme));
        assert!(code.problem.unwrap().contains("Only readme.txt"));
        assert!(code.diff.is_empty());
        assert!(
            fix_view(fix("readme.txt", "Nope", "x"), Some(readme))
                .problem
                .unwrap()
                .contains("not in")
        );
        assert!(
            fix_view(fix("readme.txt", ": ", "x"), Some(readme))
                .problem
                .unwrap()
                .contains("more than once")
        );
        assert!(fix_view(fix("readme.txt", "", "x"), Some(readme)).problem.is_some());
        assert!(fix_view(fix("readme.txt", "a", "b"), None).problem.unwrap().contains("no readme"));
    }
}
