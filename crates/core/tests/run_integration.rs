//! The run engine end to end against a local `file://` repository: dry run,
//! publish, cancel, a failing gate, resume after a trunk commit, an AI draft
//! with an applied readme fix, and an assets-only release.
//!
//! Run with `cargo test -p svnpush-core --features svn-integration`.
#![cfg(feature = "svn-integration")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use svnpush_core::ai::records::{self, ProviderRecord, ProvidersFile};
use svnpush_core::project::{AppPaths, Project, ProjectSettings};
use svnpush_core::report::{LogLine, Reporter};
use svnpush_core::run::{
    self, AiStatus, Decision, Outcome, Phase, Run, RunInputs, RunObserver, RunState,
};
use svnpush_core::secret::Secret;
use svnpush_core::settings;
use svnpush_core::svn::Svn;
use svnpush_core::vault::{CredentialStore, SvnAccount, VaultError};
use svnpush_core::{report::NullReporter, tools};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Default)]
struct MemoryVault(Mutex<HashMap<String, String>>);

impl CredentialStore for MemoryVault {
    fn get(&self, key: &str) -> Result<Option<Secret>, VaultError> {
        Ok(self.0.lock().unwrap().get(key).map(|v| Secret::new(v.clone())))
    }
    fn set(&self, key: &str, secret: &Secret) -> Result<(), VaultError> {
        self.0.lock().unwrap().insert(key.to_owned(), secret.expose().to_owned());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), VaultError> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

struct Watcher {
    tx: watch::Sender<Option<RunState>>,
    logs: Mutex<Vec<LogLine>>,
}

impl Reporter for Watcher {
    fn log(&self, line: LogLine) {
        self.logs.lock().unwrap().push(line);
    }
}

impl RunObserver for Watcher {
    fn state(&self, state: &RunState) {
        self.tx.send_replace(Some(state.clone()));
    }
}

struct Env {
    _dir: tempfile::TempDir,
    paths: AppPaths,
    project: Project,
    vault: Arc<MemoryVault>,
}

fn run_cmd(program: &str, args: &[&str]) {
    assert!(Command::new(program).args(args).status().unwrap().success(), "{program} {args:?}");
}

fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.starts_with('/') { format!("file://{text}") } else { format!("file:///{text}") }
}

fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    run_cmd("svnadmin", &["create", repo.to_str().unwrap()]);
    let url = format!("{}/minimal", file_url(&repo));
    let seeds: Vec<String> =
        ["trunk", "tags", "assets", "branches"].iter().map(|f| format!("{url}/{f}")).collect();
    let mut args = vec!["mkdir", "--parents", "-m", "Seed"];
    args.extend(seeds.iter().map(String::as_str));
    run_cmd("svn", &args);

    let plugin = dir.path().join("src/minimal");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/plugins/minimal");
    std::fs::create_dir_all(&plugin).unwrap();
    for name in ["minimal.php", "readme.txt"] {
        std::fs::copy(fixture.join(name), plugin.join(name)).unwrap();
    }
    let vault = Arc::new(MemoryVault::default());
    vault.set("svn:file:tester", &Secret::new("pw")).unwrap();
    Env {
        paths: AppPaths::new(&dir.path().join("appdata")),
        project: Project {
            path: plugin.display().to_string(),
            name: "Minimal".into(),
            svn_url: url,
            slug: "minimal".into(),
            settings: ProjectSettings::default(),
            last_release: None,
            created: "2026-09-17T00:00:00Z".into(),
        },
        vault,
        _dir: dir,
    }
}

async fn inputs(env: &Env, dry_run: bool) -> RunInputs {
    RunInputs {
        paths: env.paths.clone(),
        project: env.project.clone(),
        dry_run,
        assets_only: false,
        svn: tools::discover_svn(None).await,
        git: None,
        vault: env.vault.clone(),
        accounts: vec![SvnAccount { host: "file".into(), username: "tester".into() }],
        current_wordpress: None,
        ai: svnpush_core::ai::client::AiClient::new().unwrap(),
    }
}

struct Driver {
    rx: watch::Receiver<Option<RunState>>,
    decisions: mpsc::Sender<Decision>,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<(RunState, run::RunJournal)>,
}

async fn start(env: &Env, dry_run: bool) -> Driver {
    launch(inputs(env, dry_run).await)
}

fn launch(inputs: RunInputs) -> Driver {
    let (tx, rx) = watch::channel(None);
    let observer = Arc::new(Watcher { tx, logs: Mutex::new(Vec::new()) });
    let (decisions, receiver) = mpsc::channel(4);
    let cancel = CancellationToken::new();
    let run = Run::prepare(inputs, observer, cancel.clone(), receiver).unwrap();
    let task = tokio::spawn(run.execute());
    Driver { rx, decisions, cancel, task }
}

impl Driver {
    async fn until(&mut self, phase: Phase) -> RunState {
        loop {
            if let Some(state) = self.rx.borrow_and_update().clone() {
                if state.phase == phase {
                    return state;
                }
                assert!(
                    !state.phase.is_terminal(),
                    "ended in {:?}: {:?}",
                    state.phase,
                    state.error
                );
            }
            self.rx.changed().await.unwrap();
        }
    }

    async fn approve_prefill(&mut self) -> RunState {
        let state = self.until(Phase::AwaitingApproval).await;
        let draft = state.draft_context.clone().unwrap().prefill;
        self.decisions.send(Decision::Approve { draft }).await.unwrap();
        state
    }
}

fn read(path: &str, name: &str) -> String {
    std::fs::read_to_string(Path::new(path).join(name)).unwrap()
}

#[tokio::test]
async fn dry_run_leaves_everything_unchanged() {
    let env = env();
    let before = read(&env.project.path, "minimal.php");
    let mut driver = start(&env, true).await;
    let waiting = driver.approve_prefill().await;
    assert_eq!(waiting.draft_context.as_ref().unwrap().suggested_version, "1.0.0");
    let (state, journal) = driver.task.await.unwrap();

    assert_eq!(state.phase, Phase::DryRunComplete, "{:?}", state.error);
    assert_eq!(journal.outcome, Some(Outcome::DryRun));
    let preview = state.preview.unwrap();
    assert_eq!(preview.trunk.added, ["minimal.php", "readme.txt"]);
    assert!(Path::new(&state.package.unwrap().zip_path).is_file());
    assert_eq!(read(&env.project.path, "minimal.php"), before);
    let svn = Svn::new("svn", &NullReporter, CancellationToken::new());
    assert!(svn.status(&env.paths.working_copy("minimal")).await.unwrap().is_empty());
    assert!(svn.list_tags(&env.project.svn_url, None).await.unwrap().is_empty());
    let v16 = state.checks.iter().find(|c| c.id == "V16").unwrap();
    assert_eq!(v16.status, svnpush_core::verify::CheckStatus::Skip);
}

#[tokio::test]
async fn publish_creates_a_verified_tag() {
    let env = env();
    let mut driver = start(&env, false).await;
    driver.approve_prefill().await;
    let waiting = driver.until(Phase::AwaitingPublish).await;
    let preview = waiting.preview.unwrap();
    assert_eq!(preview.account.as_deref(), Some("tester"));
    driver
        .decisions
        .send(Decision::Publish {
            trunk_message: preview.trunk_message,
            tag_message: preview.tag_message,
        })
        .await
        .unwrap();
    let (state, journal) = driver.task.await.unwrap();

    assert_eq!(state.phase, Phase::Verified, "{:?}", state.error);
    assert_eq!(journal.outcome, Some(Outcome::Complete));
    assert!(journal.revisions.trunk.is_some() && journal.revisions.tag.is_some());
    let svn = Svn::new("svn", &NullReporter, CancellationToken::new());
    assert_eq!(svn.list_tags(&env.project.svn_url, None).await.unwrap(), ["1.0.0"]);

    // Nothing changed since the release: the run stops at the draft.
    let unchanged = start(&env, false).await;
    let (state, _) = unchanged.task.await.unwrap();
    assert_eq!(state.error.unwrap().code, "NOTHING_TO_RELEASE");

    // Releasing the same version again fails the gate and restores the files.
    let main = Path::new(&env.project.path).join("minimal.php");
    std::fs::write(&main, format!("{}\n// A change.\n", read(&env.project.path, "minimal.php")))
        .unwrap();
    let before = read(&env.project.path, "readme.txt");
    let mut again = start(&env, false).await;
    let waiting = again.until(Phase::AwaitingApproval).await;
    let mut draft = waiting.draft_context.unwrap().prefill;
    assert_eq!(draft.version, "1.0.1");
    draft.version = "1.0.0".into();
    again.decisions.send(Decision::Approve { draft }).await.unwrap();
    let (state, journal) = again.task.await.unwrap();
    assert_eq!(state.phase, Phase::Failed);
    assert_eq!(state.error.unwrap().code, "CHECKS_FAILED");
    assert_eq!(journal.outcome, Some(Outcome::Failed(run::Step::Verify)));
    let v03 = state.checks.iter().find(|c| c.id == "V03").unwrap();
    assert_eq!(v03.status, svnpush_core::verify::CheckStatus::Fail);
    assert_eq!(read(&env.project.path, "readme.txt"), before);
}

#[tokio::test]
async fn cancel_while_waiting_rolls_back() {
    let env = env();
    let before = read(&env.project.path, "minimal.php");
    let mut driver = start(&env, false).await;
    driver.approve_prefill().await;
    driver.until(Phase::AwaitingPublish).await;
    assert_ne!(read(&env.project.path, "readme.txt"), "");
    driver.cancel.cancel();
    let (state, journal) = driver.task.await.unwrap();
    assert_eq!(state.phase, Phase::Cancelled);
    assert_eq!(journal.outcome, Some(Outcome::Cancelled));
    assert_eq!(read(&env.project.path, "minimal.php"), before);
    let svn = Svn::new("svn", &NullReporter, CancellationToken::new());
    assert!(svn.status(&env.paths.working_copy("minimal")).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_second_run_is_refused_while_one_holds_the_lock() {
    let env = env();
    let mut driver = start(&env, true).await;
    driver.until(Phase::AwaitingApproval).await;
    let (_, receiver) = mpsc::channel(1);
    let observer = Arc::new(NullObserver);
    let refused =
        Run::prepare(inputs(&env, true).await, observer, CancellationToken::new(), receiver);
    assert_eq!(refused.err().unwrap().error.code, "RUN_LOCKED");
    driver.cancel.cancel();
    driver.task.await.unwrap();
}

#[tokio::test]
async fn resume_creates_the_tag_after_a_trunk_commit() {
    let env = env();
    let mut driver = start(&env, false).await;
    driver.approve_prefill().await;
    driver.until(Phase::AwaitingPublish).await;
    driver.cancel.cancel();
    let (_, mut journal) = driver.task.await.unwrap();

    // Simulate a crash between the trunk commit and the tag.
    let svn = Svn::new("svn", &NullReporter, CancellationToken::new());
    let wc = env.paths.working_copy("minimal");
    let creds =
        svnpush_core::svn::Credentials { username: "tester".into(), password: Secret::new("pw") };
    run_cmd("svn", &["update", wc.to_str().unwrap(), "-q"]);
    std::fs::write(wc.join("trunk/minimal.php"), read(&env.project.path, "minimal.php")).unwrap();
    std::fs::write(wc.join("trunk/readme.txt"), read(&env.project.path, "readme.txt")).unwrap();
    run_cmd("svn", &["add", "--force", wc.join("trunk").to_str().unwrap(), "-q"]);
    let trunk = svn.commit(&wc, "Release 1.0.0", &creds).await.unwrap();
    journal.revisions.trunk = trunk;
    journal.version = Some("1.0.0".into());
    journal.outcome = None;
    journal.save(&env.paths).unwrap();
    assert!(journal.needs_tag());

    let observer = Arc::new(NullObserver);
    let (state, journal) =
        run::resume_tag(inputs(&env, false).await, observer, CancellationToken::new(), journal)
            .await;
    assert_eq!(state.phase, Phase::Verified, "{:?}", state.error);
    assert_eq!(journal.outcome, Some(Outcome::Complete));
    assert_eq!(svn.list_tags(&env.project.svn_url, None).await.unwrap(), ["1.0.0"]);
}

struct NullObserver;

impl Reporter for NullObserver {
    fn log(&self, _line: LogLine) {}
}

impl RunObserver for NullObserver {
    fn state(&self, _state: &RunState) {}
}

impl Driver {
    async fn until_state(&mut self, wanted: impl Fn(&RunState) -> bool) -> RunState {
        loop {
            if let Some(state) = self.rx.borrow_and_update().clone() {
                if wanted(&state) {
                    return state;
                }
                assert!(
                    !state.phase.is_terminal(),
                    "ended in {:?}: {:?}",
                    state.phase,
                    state.error
                );
            }
            self.rx.changed().await.unwrap();
        }
    }
}

fn chat(content: &serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": content.to_string() }, "finish_reason": "stop" }]
    }))
}

#[tokio::test]
async fn ai_draft_and_an_applied_readme_fix_complete_a_dry_run() {
    let env = env();
    let long = format!("A fixture plugin {}.", "with a very long short description".repeat(6));
    let readme = read(&env.project.path, "readme.txt")
        .replace("A fixture plugin used by the SVNpush test suite.", &long);
    std::fs::write(Path::new(&env.project.path).join("readme.txt"), &readme).unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("release notes"))
        .respond_with(chat(&serde_json::json!({
            "version": "1.0.0", "reason": "First release.", "changelog_markdown": "* Initial release.",
            "upgrade_notice": "", "summary": "The first release."
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("release checks failed"))
        .respond_with(chat(&serde_json::json!({
            "explanation": "The short description is too long.",
            "fixes": [{ "check_id": "V07", "path": "readme.txt", "original": long, "replacement": "A fixture plugin." }]
        })))
        .mount(&server)
        .await;
    let record = ProviderRecord {
        id: "prov_local".into(),
        kind: "local".into(),
        label: "Local".into(),
        model: "gemma3".into(),
        base_url: Some(format!("{}/v1", server.uri())),
        has_key: false,
        is_default: true,
        needs_attention: None,
        created_at: String::new(),
        requests_this_month: 0,
        usage_month: String::new(),
    };
    records::save(
        &env.paths,
        &ProvidersFile { schema: 1, providers: vec![record], fallback: vec![] },
    )
    .unwrap();

    let mut driver = start(&env, true).await;
    let consent = driver
        .until_state(|s| {
            s.draft_ai.as_ref().is_some_and(|a| a.task.status == AiStatus::NeedsConsent)
        })
        .await;
    assert_eq!(consent.draft_ai.unwrap().task.privacy.unwrap().provider.id, "prov_local");
    driver
        .decisions
        .send(Decision::AcceptPrivacy { provider_id: "prov_local".into() })
        .await
        .unwrap();
    let drafted = driver
        .until_state(|s| {
            s.phase == Phase::AwaitingApproval
                && s.draft_ai.as_ref().is_some_and(|a| a.generation == 1)
        })
        .await;
    let panel = drafted.draft_ai.unwrap();
    let draft = panel.draft.clone().unwrap();
    assert_eq!(draft.summary, "The first release.");
    assert_eq!(draft.provider.as_ref().unwrap().id, "prov_local");
    assert!(
        settings::load(&env.paths).unwrap().privacy_notice_seen.contains(&"prov_local".to_owned())
    );
    driver.decisions.send(Decision::Approve { draft }).await.unwrap();

    let failing = driver.until(Phase::AwaitingFixes).await;
    let explanation = failing.explanation.unwrap();
    assert_eq!(explanation.task.status, AiStatus::Done);
    assert_eq!(explanation.fixes[0].problem, None, "{:?}", explanation.fixes[0]);
    driver.decisions.send(Decision::ApplyFixes { fixes: vec![0] }).await.unwrap();
    let (state, _) = driver.task.await.unwrap();

    assert_eq!(state.phase, Phase::DryRunComplete, "{:?}", state.error);
    assert!(state.diffs.iter().any(|d| d.diff.contains("+A fixture plugin.")));
    assert_eq!(read(&env.project.path, "readme.txt"), readme, "the dry run restores the readme");
}

fn svn_list(url: &str) -> String {
    let out = Command::new("svn").args(["ls", "--non-interactive", url]).output().unwrap();
    String::from_utf8(out.stdout).unwrap().replace("\r\n", "\n")
}

#[tokio::test]
async fn assets_only_release_commits_assets_without_a_tag() {
    let env = env();
    let assets = Path::new(&env.project.path).join(".wordpress-org");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(assets.join("banner-772x250.png"), [0x89, b'P', b'N', b'G', 1, 2, 3]).unwrap();
    std::fs::write(assets.join("icon-128x128.png"), [0x89, b'P', b'N', b'G', 4, 5, 6]).unwrap();
    let before = read(&env.project.path, "minimal.php");

    let mut assets_inputs = inputs(&env, false).await;
    assets_inputs.assets_only = true;
    let mut driver = launch(assets_inputs);
    let waiting = driver.until(Phase::AwaitingPublish).await;
    assert!(waiting.assets_only);
    let preview = waiting.preview.unwrap();
    assert_eq!(preview.assets.added, ["banner-772x250.png", "icon-128x128.png"]);
    assert!(preview.trunk.added.is_empty() && preview.tag_url.is_empty());
    let ids: Vec<&str> = waiting.checks.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["V14", "V16", "W04", "V15"]);
    for skipped in [run::Step::Draft, run::Step::Write, run::Step::Build] {
        let view = waiting.steps.iter().find(|s| s.step == skipped).unwrap();
        assert_eq!(view.status, run::StepStatus::Skipped);
    }
    driver
        .decisions
        .send(Decision::Publish {
            trunk_message: preview.trunk_message,
            tag_message: String::new(),
        })
        .await
        .unwrap();
    let (state, journal) = driver.task.await.unwrap();

    assert_eq!(state.phase, Phase::Verified, "{:?}", state.error);
    assert!(journal.assets_only && journal.revisions.assets.is_some());
    assert!(journal.revisions.trunk.is_none() && journal.revisions.tag.is_none());
    assert_eq!(state.publish.unwrap().assets_revision, journal.revisions.assets);
    let url = &env.project.svn_url;
    assert_eq!(svn_list(&format!("{url}/assets")), "banner-772x250.png\nicon-128x128.png\n");
    assert_eq!(svn_list(&format!("{url}/trunk")), "");
    assert_eq!(svn_list(&format!("{url}/tags")), "");
    assert_eq!(read(&env.project.path, "minimal.php"), before);

    let mut again_inputs = inputs(&env, false).await;
    again_inputs.assets_only = true;
    let (state, _) = launch(again_inputs).task.await.unwrap();
    assert_eq!(state.error.unwrap().code, "NOTHING_TO_RELEASE");
}
