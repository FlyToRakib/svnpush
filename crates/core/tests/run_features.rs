//! Engine features end to end against a local `file://` repository: the AI
//! draft with an applied readme fix, the assets-only release, and the Step 5
//! file check with its proposed `.distignore`.
//!
//! Run with `cargo test -p svnpush-core --features svn-integration`.
#![cfg(feature = "svn-integration")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::Path;
use std::process::Command;

use support::{env, inputs, launch, launch_with, read, start};
use svnpush_core::ai::records::{self, ProviderRecord, ProvidersFile};
use svnpush_core::run::files::ReviewReason;
use svnpush_core::run::{self, AiStatus, Decision, Phase};
use svnpush_core::settings;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

#[tokio::test]
async fn the_file_check_proposes_a_distignore_and_saves_it() {
    let env = env();
    let plugin = Path::new(&env.project.path);
    std::fs::create_dir_all(plugin.join(".agent")).unwrap();
    std::fs::write(plugin.join(".agent/notes.md"), "private notes").unwrap();
    std::fs::create_dir_all(plugin.join("docs")).unwrap();
    std::fs::write(plugin.join("docs/guide.md"), "developer guide").unwrap();

    let mut driver = launch_with(inputs(&env, true).await, false);
    driver.approve_prefill().await;
    let waiting = driver.until(Phase::AwaitingFileReview).await;
    let review = waiting.file_review.unwrap();
    assert_eq!(review.reasons, [ReviewReason::NoDistignore, ReviewReason::FirstRelease]);
    assert!(review.editable && !review.distignore_exists);
    assert!(review.distignore.lines().any(|l| l == ".*"));
    let released: Vec<&str> = review.preview.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(released, ["minimal.php", "readme.txt"]);
    assert!(review.preview.excluded.contains(&".agent/".to_owned()));
    assert!(review.preview.excluded.contains(&"docs/".to_owned()));

    let text = format!("{}/extra\n", review.distignore);
    driver.decisions.send(Decision::ConfirmFiles { distignore: Some(text) }).await.unwrap();
    let (state, _) = driver.task.await.unwrap();
    assert_eq!(state.phase, Phase::DryRunComplete, "{:?}", state.error);
    let saved = read(&env.project.path, ".distignore");
    assert!(saved.contains(".*\n") && saved.ends_with("/extra\n"));
    let entries =
        svnpush_core::package::zip_entry_names(Path::new(&state.package.unwrap().zip_path))
            .unwrap();
    assert!(entries.iter().all(|e| !e.contains(".agent") && !e.contains("docs/")), "{entries:?}");

    // With a .distignore in place, only the first-release reason remains.
    let mut again = launch_with(inputs(&env, true).await, false);
    again.approve_prefill().await;
    let review = again.until(Phase::AwaitingFileReview).await.file_review.unwrap();
    assert_eq!(review.reasons, [ReviewReason::FirstRelease]);
    assert!(review.distignore_exists);
    again.cancel.cancel();
    let _ = again.task.await;
}
