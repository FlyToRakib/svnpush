//! Shared helpers for the engine suites: a local `file://` repository, an
//! in-memory keychain, and a driver that answers the run's questions.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use svnpush_core::project::{AppPaths, Project, ProjectSettings};
use svnpush_core::report::{LogLine, Reporter};
use svnpush_core::run::{self, Decision, Phase, Run, RunInputs, RunObserver, RunState};
use svnpush_core::secret::Secret;
use svnpush_core::tools;
use svnpush_core::vault::{CredentialStore, SvnAccount, VaultError};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct MemoryVault(Mutex<HashMap<String, String>>);

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

pub struct Watcher {
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

pub struct Env {
    _dir: tempfile::TempDir,
    pub paths: AppPaths,
    pub project: Project,
    pub vault: Arc<MemoryVault>,
}

pub fn run_cmd(program: &str, args: &[&str]) {
    assert!(Command::new(program).args(args).status().unwrap().success(), "{program} {args:?}");
}

pub fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.starts_with('/') { format!("file://{text}") } else { format!("file:///{text}") }
}

pub fn env() -> Env {
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

pub async fn inputs(env: &Env, dry_run: bool) -> RunInputs {
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

pub struct Driver {
    pub rx: watch::Receiver<Option<RunState>>,
    pub decisions: mpsc::Sender<Decision>,
    pub cancel: CancellationToken,
    pub task: tokio::task::JoinHandle<(RunState, run::RunJournal)>,
}

pub async fn start(env: &Env, dry_run: bool) -> Driver {
    launch(inputs(env, dry_run).await)
}

pub fn launch(inputs: RunInputs) -> Driver {
    launch_with(inputs, true)
}

/// Starts a run. With `accept_files`, the Step 5 file check is confirmed
/// unchanged whenever it appears, for tests that are not about it.
pub fn launch_with(inputs: RunInputs, accept_files: bool) -> Driver {
    let (tx, rx) = watch::channel(None);
    let observer = Arc::new(Watcher { tx, logs: Mutex::new(Vec::new()) });
    let (decisions, receiver) = mpsc::channel(4);
    let cancel = CancellationToken::new();
    let run = Run::prepare(inputs, observer, cancel.clone(), receiver).unwrap();
    let task = tokio::spawn(run.execute());
    if accept_files {
        let mut watch = rx.clone();
        let answers = decisions.clone();
        tokio::spawn(async move {
            while watch.changed().await.is_ok() {
                let phase = watch.borrow_and_update().as_ref().map(|s| s.phase);
                if phase == Some(Phase::AwaitingFileReview) {
                    let _ = answers.send(Decision::ConfirmFiles { distignore: None }).await;
                }
            }
        });
    }
    Driver { rx, decisions, cancel, task }
}

impl Driver {
    pub async fn until(&mut self, phase: Phase) -> RunState {
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

    pub async fn approve_prefill(&mut self) -> RunState {
        let state = self.until(Phase::AwaitingApproval).await;
        let draft = state.draft_context.clone().unwrap().prefill;
        self.decisions.send(Decision::Approve { draft }).await.unwrap();
        state
    }
}

pub fn read(path: &str, name: &str) -> String {
    std::fs::read_to_string(Path::new(path).join(name)).unwrap()
}

pub struct NullObserver;

impl Reporter for NullObserver {
    fn log(&self, _line: LogLine) {}
}

impl RunObserver for NullObserver {
    fn state(&self, _state: &RunState) {}
}

impl Driver {
    pub async fn until_state(&mut self, wanted: impl Fn(&RunState) -> bool) -> RunState {
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
