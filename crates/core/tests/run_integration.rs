//! The run engine end to end against a local `file://` repository: dry run,
//! publish, cancel, a failing gate, the project lock, and resume after a
//! trunk commit.
//!
//! Run with `cargo test -p svnpush-core --features svn-integration`.
#![cfg(feature = "svn-integration")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::Path;
use std::sync::Arc;

use support::{NullObserver, env, inputs, read, run_cmd, start};
use svnpush_core::report::NullReporter;
use svnpush_core::run::{self, Decision, Outcome, Phase, Run};
use svnpush_core::secret::Secret;
use svnpush_core::svn::Svn;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

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
