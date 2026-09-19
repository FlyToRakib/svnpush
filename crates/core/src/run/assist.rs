//! The AI inside a run: resolving the provider, the privacy notice, running a
//! task while the developer can still act, and the Step 2 draft (plan §5.2, §9.8, §12.4).

use std::convert::Infallible;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use crate::ai::adapters::revoye::REVOYE;
use crate::ai::client::{AiClient, ClientError};
use crate::ai::prompts::{self, DraftMaterial, Prompt};
use crate::ai::provider::{Adapter, Fleet, WorkId};
use crate::ai::records::{self, ProviderRecord, ProvidersFile};
use crate::ai::router::{self, Ask, RouteEvent, Router};
use crate::ai::schemas::{self, DraftAnswer, FileSummary};
use crate::ai::task::{self, Answer, JSON_MAX_TOKENS, TaskError};
use crate::detect::PluginFacts;
use crate::project::{AiChoice, AppPaths};
use crate::readme::ChangelogEntry;
use crate::settings;
use crate::vault::{self, CredentialStore};
use crate::version::Version;

use super::RunFailure;
use super::ai_view::{AiStatus, AiTask, DraftAi, PrivacyNotice, SummaryProgress};
use super::engine::Run;
use super::material::{MAX_SUMMARISED_FILES, Material};
use super::model::{
    Decision, DraftProvider, ErrorView, Phase, ReleaseDraft, RunState, Step, StepStatus,
};

/// How often the Revoye fleet line refreshes while a job waits.
const FLEET_REFRESH: Duration = Duration::from_secs(60);

/// Progress sent from a running AI task to the run.
pub(super) enum AssistEvent {
    Route(RouteEvent),
    Fleet(Fleet),
    Summaries(SummaryProgress),
}

pub(super) type Events = UnboundedSender<AssistEvent>;

/// Which provider would run.
pub(super) enum Resolved {
    Off,
    NoProvider(ErrorView),
    Ready(ProviderRecord),
}

/// Everything an AI task needs, owned, so it can run while the run waits for decisions.
pub(super) struct Job {
    paths: AppPaths,
    vault: Arc<dyn CredentialStore>,
    client: AiClient,
    providers: ProvidersFile,
    record: ProviderRecord,
    slug: String,
    version: String,
}

impl Job {
    pub(super) async fn ask<T: DeserializeOwned>(
        &self,
        prompt: &Prompt,
        schema: &Value,
        task_name: &str,
        cancel: &CancellationToken,
        events: &Events,
    ) -> Result<Answer<T>, TaskError> {
        let router =
            Router { paths: &self.paths, vault: self.vault.as_ref(), client: &self.client };
        let ask = Ask {
            system: &prompt.system,
            user: &prompt.user,
            json_schema: Some(schema),
            max_tokens: JSON_MAX_TOKENS,
            work: Some(WorkId { slug: &self.slug, version: &self.version, task: task_name }),
        };
        let sender = events.clone();
        let on_event = move |event| {
            // The run may have stopped listening; progress is then irrelevant.
            let _ = sender.send(AssistEvent::Route(event));
        };
        task::ask_json(&router, &self.providers, &self.record, &ask, schema, cancel, &on_event)
            .await
    }

    /// Refreshes the Revoye fleet line once a minute; never finishes.
    pub(super) async fn watch_fleet(&self, events: &Events) -> Infallible {
        if self.record.kind != REVOYE.meta().kind {
            return std::future::pending().await;
        }
        loop {
            if let Ok(Some(key)) = self.vault.get(&vault::ai_key(&self.record.id))
                && let Ok(fleet) = self.client.fleet_status(&REVOYE, key.expose()).await
            {
                let _ = events.send(AssistEvent::Fleet(fleet));
            }
            tokio::time::sleep(FLEET_REFRESH).await;
        }
    }
}

/// The version to use from an AI suggestion: overridden to the next patch when
/// it is invalid or not newer than the previous release (plan §12.4).
pub(super) fn settle_version(
    suggested: &str,
    previous: Option<&str>,
    fallback: &str,
) -> (String, Option<String>) {
    let previous_version = previous.and_then(|p| Version::parse(p).ok());
    let acceptable = Version::parse(suggested.trim())
        .is_ok_and(|v| previous_version.as_ref().is_none_or(|p| &v > p));
    if acceptable {
        return (suggested.trim().to_owned(), None);
    }
    let next = previous_version.map_or_else(|| fallback.to_owned(), |p| p.next_patch().to_string());
    let reason = previous.map_or_else(
        || "is not a valid version".to_owned(),
        |p| format!("is not a valid version newer than {p}"),
    );
    (
        next.clone(),
        Some(format!("The AI suggested \"{suggested}\", which {reason}. Using {next} instead.")),
    )
}

fn draft_task(state: &mut RunState) -> Option<&mut AiTask> {
    state.draft_ai.as_mut().map(|d| &mut d.task)
}

struct About {
    name: String,
    slug: String,
    previous: Option<String>,
    entries: Vec<ChangelogEntry>,
    commits: Vec<String>,
}

async fn write_draft(
    job: Job,
    material: Material,
    about: About,
    cancel: CancellationToken,
    events: Events,
) -> Result<(Answer<DraftAnswer>, bool), TaskError> {
    let work = async {
        let from_summaries = material.needs_summaries();
        let changes = if from_summaries {
            let total =
                u32::try_from(material.diffs.len().min(MAX_SUMMARISED_FILES)).unwrap_or(u32::MAX);
            let mut lines = Vec::new();
            for (index, (path, diff)) in material.diffs.iter().enumerate() {
                if index >= MAX_SUMMARISED_FILES {
                    lines.push(format!("{path}: not summarised (file limit reached)"));
                    continue;
                }
                let done = u32::try_from(index).unwrap_or(u32::MAX);
                let _ = events.send(AssistEvent::Summaries(SummaryProgress { done, total }));
                let answer: Answer<FileSummary> = job
                    .ask(
                        &prompts::summarise_file(path, diff),
                        &schemas::SUMMARISE_FILE,
                        "summarise_file",
                        &cancel,
                        &events,
                    )
                    .await?;
                lines.push(format!("{path}: {}", answer.value.summary));
            }
            let _ = events.send(AssistEvent::Summaries(SummaryProgress { done: total, total }));
            lines.join("\n")
        } else {
            material.joined()
        };
        let prompt = prompts::draft_release(&DraftMaterial {
            name: &about.name,
            slug: &about.slug,
            previous: about.previous.as_deref(),
            recent_entries: &about.entries,
            commits: &about.commits,
            stat: &material.stat,
            changes: &changes,
            from_summaries,
        });
        let answer =
            job.ask(&prompt, &schemas::DRAFT_RELEASE, "draft_release", &cancel, &events).await?;
        Ok((answer, from_summaries))
    };
    tokio::select! {
        result = work => result,
        never = job.watch_fleet(&events) => match never {},
    }
}

impl Run {
    fn provider_file(&self) -> ProvidersFile {
        records::load(&self.inputs.paths).unwrap_or_else(|e| {
            self.observer.info(&format!("Could not read the AI provider records: {e}"));
            ProvidersFile { schema: 1, providers: Vec::new(), fallback: Vec::new() }
        })
    }

    pub(super) fn provider_choices(&self) -> Vec<DraftProvider> {
        self.provider_file().providers.iter().map(DraftProvider::from).collect()
    }

    /// The provider for this run: the Change link's choice, then the project's.
    pub(super) fn resolve_ai(&self, override_id: Option<&str>) -> Resolved {
        let pinned = match &self.inputs.project.settings.ai_provider {
            AiChoice::Off => return Resolved::Off,
            AiChoice::Pinned { provider_id } => Some(provider_id.as_str()),
            AiChoice::Default => None,
        };
        let chosen = override_id.or(self.ai_override.as_deref());
        match router::resolve(&self.provider_file(), chosen, pinned) {
            Ok(record) => Resolved::Ready(record),
            Err(e) => Resolved::NoProvider(ErrorView::from_coded(&e)),
        }
    }

    /// A Generate decision: an explicit provider becomes the run's choice.
    pub(super) fn choose_ai(&mut self, provider_id: Option<String>) -> Resolved {
        if provider_id.is_some() {
            self.ai_override = provider_id;
        }
        self.resolve_ai(None)
    }

    pub(super) fn job(&self, record: ProviderRecord, version: &str) -> Job {
        Job {
            paths: self.inputs.paths.clone(),
            vault: self.inputs.vault.clone(),
            client: self.inputs.ai.clone(),
            providers: self.provider_file(),
            record,
            slug: self.inputs.project.slug.clone(),
            version: version.to_owned(),
        }
    }

    /// Whether the developer has accepted the notice for this provider.
    pub(super) fn privacy_seen(&self, provider_id: &str) -> bool {
        settings::load(&self.inputs.paths)
            .is_ok_and(|s| s.privacy_notice_seen.iter().any(|id| id == provider_id))
    }

    pub(super) fn privacy_notice(record: &ProviderRecord) -> PrivacyNotice {
        PrivacyNotice { provider: record.into(), revoye: record.kind == REVOYE.meta().kind }
    }

    pub(super) fn accept_privacy(&mut self, provider_id: &str) {
        let mut app = settings::load(&self.inputs.paths).unwrap_or_default();
        if !app.privacy_notice_seen.iter().any(|id| id == provider_id) {
            app.privacy_notice_seen.push(provider_id.to_owned());
        }
        if let Err(e) = settings::save(&self.inputs.paths, &app) {
            self.state.notices.push(format!("Could not remember the accepted privacy notice: {e}"));
        }
    }

    /// Runs an AI task to completion while applying its progress and taking
    /// decisions. A decision other than accepting a notice stops the task
    /// (cancelling a Revoye job on the server) and is handed back.
    pub(super) async fn pump<T>(
        &mut self,
        work: impl Future<Output = T>,
        mut events: UnboundedReceiver<AssistEvent>,
        stop: &CancellationToken,
        task_of: fn(&mut RunState) -> Option<&mut AiTask>,
    ) -> Result<(T, Option<Decision>), RunFailure> {
        tokio::pin!(work);
        let mut pending = None;
        let mut cancelled = false;
        loop {
            tokio::select! {
                result = &mut work => {
                    while let Ok(event) = events.try_recv() {
                        self.apply_event(task_of, event);
                    }
                    return if cancelled { Err(RunFailure::cancelled()) } else { Ok((result, pending)) };
                }
                () = self.cancel.cancelled(), if !cancelled => {
                    cancelled = true;
                    stop.cancel();
                }
                Some(event) = events.recv() => {
                    self.apply_event(task_of, event);
                    self.emit();
                }
                decision = self.decisions.recv(), if pending.is_none() && !cancelled => match decision {
                    Some(Decision::AcceptPrivacy { .. }) => {}
                    Some(other) => {
                        pending = Some(other);
                        stop.cancel();
                    }
                    None => {
                        cancelled = true;
                        stop.cancel();
                    }
                },
            }
        }
    }

    fn apply_event(
        &mut self,
        task_of: fn(&mut RunState) -> Option<&mut AiTask>,
        event: AssistEvent,
    ) {
        let Some(task) = task_of(&mut self.state) else { return };
        match event {
            AssistEvent::Route(RouteEvent::Attempt(record)) => {
                task.provider = Some((&record).into());
                task.job_status = None;
                task.queue_position = None;
            }
            AssistEvent::Route(RouteEvent::Pending(pending)) => {
                task.job_status = Some(pending.status);
                task.queue_position = pending.queue_position;
            }
            AssistEvent::Fleet(fleet) => task.fleet = Some(fleet),
            AssistEvent::Summaries(progress) => task.summaries = Some(progress),
        }
    }

    /// Step 2 after the change set: the AI draft, regenerate, Change, manual, approve.
    pub(super) async fn decide_draft(
        &mut self,
        facts: &PluginFacts,
        material: &Material,
        suggested: &str,
    ) -> Result<ReleaseDraft, RunFailure> {
        let mut task = AiTask::new(AiStatus::Manual, self.provider_choices());
        let mut next = None;
        match self.resolve_ai(None) {
            Resolved::Off => task.status = AiStatus::Off,
            Resolved::NoProvider(error) => {
                task.status = AiStatus::NoProvider;
                task.error = Some(error);
            }
            Resolved::Ready(_) => next = Some(Decision::Generate { provider_id: None }),
        }
        self.state.draft_ai =
            Some(DraftAi { task, draft: None, generation: 0, from_summaries: false, notice: None });

        loop {
            let decision = match next.take() {
                Some(decision) => decision,
                None => self.wait_for(Step::Draft, Phase::AwaitingApproval, Some).await?,
            };
            match decision {
                Decision::Approve { draft } => return Ok(draft),
                Decision::Manual => {
                    if let Some(task) = draft_task(&mut self.state) {
                        task.status = AiStatus::Manual;
                        task.privacy = None;
                    }
                }
                Decision::AcceptPrivacy { provider_id } => {
                    self.accept_privacy(&provider_id);
                    next = Some(Decision::Generate { provider_id: Some(provider_id) });
                }
                Decision::Generate { provider_id } => match self.choose_ai(provider_id) {
                    Resolved::Off => {}
                    Resolved::NoProvider(error) => {
                        if let Some(task) = draft_task(&mut self.state) {
                            task.status = AiStatus::NoProvider;
                            task.error = Some(error);
                        }
                    }
                    Resolved::Ready(record) if !self.privacy_seen(&record.id) => {
                        if let Some(task) = draft_task(&mut self.state) {
                            task.status = AiStatus::NeedsConsent;
                            task.provider = Some((&record).into());
                            task.privacy = Some(Self::privacy_notice(&record));
                        }
                    }
                    Resolved::Ready(record) => {
                        next = self.generate_draft(record, facts, material, suggested).await?;
                    }
                },
                Decision::ApplyFixes { .. }
                | Decision::Stop
                | Decision::Publish { .. }
                | Decision::ConfirmFiles { .. } => {}
            }
        }
    }

    async fn generate_draft(
        &mut self,
        record: ProviderRecord,
        facts: &PluginFacts,
        material: &Material,
        suggested: &str,
    ) -> Result<Option<Decision>, RunFailure> {
        self.state.phase = Phase::Drafting;
        self.state.set_step(Step::Draft, StepStatus::Running, None);
        if let Some(task) = draft_task(&mut self.state) {
            task.start((&record).into());
        }
        self.emit();

        let about = About {
            name: facts.name.clone(),
            slug: facts.slug.clone(),
            previous: self.state.previous.clone(),
            entries: facts
                .readme
                .as_ref()
                .map(|r| r.changelog.iter().take(3).cloned().collect())
                .unwrap_or_default(),
            commits: self
                .state
                .draft_context
                .as_ref()
                .map(|c| c.changes.commits.clone())
                .unwrap_or_default(),
        };
        let job = self.job(record, suggested);
        let (sender, receiver) = mpsc::unbounded_channel();
        let stop = self.cancel.child_token();
        let work = write_draft(job, material.clone(), about, stop.clone(), sender);
        let (result, pending) = self.pump(work, receiver, &stop, draft_task).await?;

        let previous = self.state.previous.clone();
        let Some(panel) = self.state.draft_ai.as_mut() else { return Ok(pending) };
        match result {
            Ok((answer, from_summaries)) => {
                let value = answer.value;
                let (version, notice) =
                    settle_version(&value.version, previous.as_deref(), suggested);
                panel.draft = Some(ReleaseDraft {
                    version,
                    reason: value.reason,
                    changelog_markdown: value.changelog_markdown,
                    upgrade_notice: value.upgrade_notice,
                    summary: value.summary,
                    provider: Some((&answer.routed.provider).into()),
                    fell_back_from: answer.routed.fell_back_from,
                });
                panel.generation += 1;
                panel.from_summaries = from_summaries;
                panel.notice = notice;
                panel.task.status = AiStatus::Done;
                panel.task.summaries = None;
                panel.task.job_status = None;
                panel.task.queue_position = None;
            }
            Err(TaskError::Client(ClientError::Cancelled)) => panel.task.status = AiStatus::Manual,
            Err(error) => {
                panel.task.status = AiStatus::Failed;
                panel.task.error = Some(ErrorView::from_coded(&error));
                panel.task.raw_text = match &error {
                    TaskError::Invalid { raw_text, .. } | TaskError::Truncated { raw_text, .. } => {
                        Some(raw_text.clone())
                    }
                    TaskError::Client(_) | TaskError::Refused { .. } => None,
                };
                panel.task.job_status = None;
                panel.task.queue_position = None;
            }
        }
        self.emit();
        Ok(pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_not_newer_than_the_previous_release_are_overridden() {
        assert_eq!(settle_version("1.3.0", Some("1.2.0"), "1.2.1"), ("1.3.0".to_owned(), None));
        let (version, notice) = settle_version("1.2.0", Some("1.2.0"), "1.2.1");
        assert_eq!(version, "1.2.1");
        assert!(notice.unwrap().contains("newer than 1.2.0"));
        assert_eq!(settle_version("1.1", Some("1.2.0"), "x").0, "1.2.1");
        assert_eq!(settle_version("soon", None, "1.0.0").0, "1.0.0");
        assert_eq!(settle_version(" 2.0 ", None, "1.0.0"), ("2.0".to_owned(), None));
    }
}
