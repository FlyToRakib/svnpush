//! The run engine end to end against a local `file://` repository: dry run,
//! publish, cancel, a failing gate, and resume after a trunk commit.
//!
//! Run with `cargo test -p svnpush-core --features svn-integration`.
#![cfg(feature = "svn-integration")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use svnpush_core::project::{AppPaths, Project, ProjectSettings};
use svnpush_core::report::{LogLine, Reporter};
use svnpush_core::run::{self, Decision, Outcome, Phase, Run, RunInputs, RunObserver, RunState};
use svnpush_core::secret::Secret;
use svnpush_core::svn::Svn;
use svnpush_core::vault::{CredentialStore, SvnAccount, VaultError};
use svnpush_core::{report::NullReporter, tools};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

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
        svn: tools::discover_svn(None).await,
        git: None,
        vault: env.vault.clone(),
        accounts: vec![SvnAccount { host: "file".into(), username: "tester".into() }],
        current_wordpress: None,
    }
}

struct Driver {
    rx: watch::Receiver<Option<RunState>>,
    decisions: mpsc::Sender<Decision>,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<(RunState, run::RunJournal)>,
}

async fn start(env: &Env, dry_run: bool) -> Driver {
    let (tx, rx) = watch::channel(None);
    let observer = Arc::new(Watcher { tx, logs: Mutex::new(Vec::new()) });
    let (decisions, receiver) = mpsc::channel(4);
    let cancel = CancellationToken::new();
    let run = Run::prepare(inputs(env, dry_run).await, observer, cancel.clone(), receiver).unwrap();
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
