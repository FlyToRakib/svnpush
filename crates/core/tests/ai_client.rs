//! The client and router against a mock server: the Revoye job lifecycle,
//! cancellation, a poll rate limit, fallback, and a schema-invalid answer
//! followed by a retry (plan §15).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use svnpush_core::ai::adapters::revoye::REVOYE;
use svnpush_core::ai::client::{AiClient, ClientError, Pending};
use svnpush_core::ai::provider::{ErrorCode, GenerateRequest, Message, Role, WorkId};
use svnpush_core::ai::records::{self, ProviderRecord, ProvidersFile};
use svnpush_core::ai::router::{Ask, RouteEvent, Router};
use svnpush_core::ai::schemas::{self, DraftAnswer};
use svnpush_core::ai::task::{self, TaskError};
use svnpush_core::project::AppPaths;
use svnpush_core::secret::Secret;
use svnpush_core::vault::{self, CredentialStore, VaultError};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const JOB: &str = "job_01JAY7Q2K8XYZ";

fn job(status: &str, response: &Value, extra: Value) -> Value {
    let mut body = json!({
        "id": JOB, "status": status, "response": response, "provider": null, "agent_id": null,
        "agent_name": null, "conversation_ref": null, "attempts": 1, "queue_ms": null, "run_ms": null,
        "created_at": "2026-09-17T10:00:00Z", "finished_at": null, "content_pruned_at": null,
        "metadata": { "source": "svnpush", "task": "draft_release" }
    });
    if let (Value::Object(target), Value::Object(more)) = (&mut body, extra) {
        target.extend(more);
    }
    body
}

/// Answers each call with the next response in the list, repeating the last.
struct Sequence(Mutex<Vec<ResponseTemplate>>);

impl Respond for Sequence {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let mut all = self.0.lock().unwrap();
        if all.len() > 1 { all.remove(0) } else { all[0].clone() }
    }
}

fn sequence(responses: Vec<ResponseTemplate>) -> Sequence {
    Sequence(Mutex::new(responses))
}

fn revoye_client(server: &MockServer) -> AiClient {
    AiClient::new().unwrap().with_url_rewrite("https://api.revoye.com", &server.uri())
}

fn revoye_request(messages: &[Message]) -> GenerateRequest<'_> {
    GenerateRequest {
        base_url: None,
        api_key: Some("revoye_sk_test_abc"),
        model: "revoye/auto",
        system: None,
        messages,
        max_tokens: None,
        temperature: None,
        json_schema: None,
        work: Some(WorkId { slug: "demo", version: "1.2.0", task: "draft_release" }),
    }
}

#[tokio::test]
async fn revoye_submit_poll_until_succeeded() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/completions"))
        .and(header("authorization", "Bearer revoye_sk_test_abc"))
        .respond_with(ResponseTemplate::new(202).set_body_json(job(
            "queued",
            &Value::Null,
            json!({ "queue_position": 2 }),
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/completions/{JOB}")))
        .respond_with(sequence(vec![
            ResponseTemplate::new(200).set_body_json(job("dispatched", &Value::Null, json!({}))),
            ResponseTemplate::new(200).set_body_json(job(
                "succeeded",
                &json!("The answer."),
                json!({}),
            )),
        ]))
        .mount(&server)
        .await;

    let seen: Arc<Mutex<Vec<Pending>>> = Arc::default();
    let record = seen.clone();
    let messages = [Message { role: Role::User, content: "Hi".into() }];
    let result = revoye_client(&server)
        .generate(&REVOYE, &revoye_request(&messages), &CancellationToken::new(), &move |p| {
            record.lock().unwrap().push(p.clone());
        })
        .await
        .unwrap();
    assert_eq!(result.text, "The answer.");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].queue_position, Some(2));
    assert_eq!(seen[0].status, "queued");
    assert_eq!(seen[1].status, "dispatched");
}

#[tokio::test]
async fn cancelling_a_pending_job_deletes_it_on_the_server() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/completions"))
        .respond_with(ResponseTemplate::new(202).set_body_json(job(
            "queued",
            &Value::Null,
            json!({ "queue_position": 5 }),
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/completions/{JOB}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(job(
            "queued",
            &Value::Null,
            json!({}),
        )))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("/v1/completions/{JOB}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(job(
            "cancelled",
            &Value::Null,
            json!({}),
        )))
        .expect(1)
        .mount(&server)
        .await;

    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let messages = [Message { role: Role::User, content: "Hi".into() }];
    let err = revoye_client(&server)
        .generate(&REVOYE, &revoye_request(&messages), &cancel, &move |_| trigger.cancel())
        .await
        .unwrap_err();
    assert_eq!(err, ClientError::Cancelled);
}

#[tokio::test]
async fn a_poll_rate_limit_waits_for_retry_after_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/completions"))
        .respond_with(ResponseTemplate::new(202).set_body_json(job(
            "queued",
            &Value::Null,
            json!({}),
        )))
        .mount(&server)
        .await;
    let limited = ResponseTemplate::new(429)
        .insert_header("Retry-After", "1")
        .set_body_json(json!({ "error": { "code": "RATE_LIMITED", "message": "slow down", "request_id": "req_1", "details": {} } }));
    Mock::given(method("GET"))
        .and(path(format!("/v1/completions/{JOB}")))
        .respond_with(sequence(vec![
            limited,
            ResponseTemplate::new(200).set_body_json(job("succeeded", &json!("ok"), json!({}))),
        ]))
        .mount(&server)
        .await;
    let messages = [Message { role: Role::User, content: "Hi".into() }];
    let started = std::time::Instant::now();
    let result = revoye_client(&server)
        .generate(&REVOYE, &revoye_request(&messages), &CancellationToken::new(), &|_| {})
        .await
        .unwrap();
    assert_eq!(result.text, "ok");
    assert!(
        started.elapsed() >= Duration::from_secs(9),
        "two poll intervals plus the Retry-After wait"
    );
}

#[tokio::test]
async fn submit_errors_use_the_error_envelope() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({ "error": { "code": "UNAUTHORIZED", "message": "x", "request_id": "r", "details": {} } })))
        .mount(&server)
        .await;
    let messages = [Message { role: Role::User, content: "Hi".into() }];
    let err = revoye_client(&server)
        .generate(&REVOYE, &revoye_request(&messages), &CancellationToken::new(), &|_| {})
        .await
        .unwrap_err();
    match err {
        ClientError::Provider(e) => assert_eq!(e.code, ErrorCode::Auth),
        ClientError::Cancelled => panic!("not cancelled"),
    }
}

// ── Router and JSON task against an OpenAI-compatible mock ───────────────

#[derive(Default)]
struct MemoryVault(Mutex<HashMap<String, String>>);

impl CredentialStore for MemoryVault {
    fn get(&self, key: &str) -> Result<Option<Secret>, VaultError> {
        Ok(self.0.lock().unwrap().get(key).map(|v| Secret::new(v.clone())))
    }
    fn set(&self, key: &str, secret: &Secret) -> Result<(), VaultError> {
        self.0.lock().unwrap().insert(key.into(), secret.expose().into());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), VaultError> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

fn chat(content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": content }, "finish_reason": "stop" }]
    }))
}

fn record(id: &str, kind: &str, base: &str) -> ProviderRecord {
    ProviderRecord {
        id: id.into(),
        kind: kind.into(),
        label: id.into(),
        model: "test-model".into(),
        base_url: Some(base.into()),
        has_key: kind != "local",
        is_default: false,
        needs_attention: None,
        created_at: String::new(),
        requests_this_month: 0,
        usage_month: String::new(),
    }
}

const GOOD: &str = r#"{"version":"1.2.0","reason":"New export.","changelog_markdown":"* Added export.","upgrade_notice":"","summary":"Adds export."}"#;

#[tokio::test]
async fn schema_invalid_answer_is_retried_with_the_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sequence(vec![chat(r#"{"version":"1.2.0"}"#), chat(GOOD)]))
        .expect(2)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let first = record("prov_local", "local", &format!("{}/v1", server.uri()));
    let file = ProvidersFile { schema: 1, providers: vec![first.clone()], fallback: vec![] };
    records::save(&paths, &file).unwrap();
    let vault = MemoryVault::default();
    let client = AiClient::new().unwrap();
    let router = Router { paths: &paths, vault: &vault, client: &client };
    let ask = Ask {
        system: "s",
        user: "u",
        json_schema: Some(&schemas::DRAFT_RELEASE),
        max_tokens: 1000,
        work: None,
    };

    let answer: task::Answer<DraftAnswer> = task::ask_json(
        &router,
        &file,
        &first,
        &ask,
        &schemas::DRAFT_RELEASE,
        &CancellationToken::new(),
        &|_| {},
    )
    .await
    .unwrap();
    assert_eq!(answer.value.version, "1.2.0");
    let requests = server.received_requests().await.unwrap();
    let retry: Value = serde_json::from_slice(&requests[1].body).unwrap();
    let text = retry["messages"][1]["content"].as_str().unwrap();
    assert!(text.contains("could not be used"), "{text}");
    assert_eq!(records::load(&paths).unwrap().providers[0].requests_this_month, 2);
}

#[tokio::test]
async fn two_invalid_answers_return_the_raw_text() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(chat("Sorry, no JSON."))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let first = record("prov_local", "local", &format!("{}/v1", server.uri()));
    let file = ProvidersFile { schema: 1, providers: vec![first.clone()], fallback: vec![] };
    let vault = MemoryVault::default();
    let client = AiClient::new().unwrap();
    let router = Router { paths: &paths, vault: &vault, client: &client };
    let ask = Ask {
        system: "s",
        user: "u",
        json_schema: Some(&schemas::DRAFT_RELEASE),
        max_tokens: 1000,
        work: None,
    };
    let err = task::ask_json::<DraftAnswer>(
        &router,
        &file,
        &first,
        &ask,
        &schemas::DRAFT_RELEASE,
        &CancellationToken::new(),
        &|_| {},
    )
    .await
    .unwrap_err();
    match err {
        TaskError::Invalid { raw_text, .. } => assert_eq!(raw_text, "Sorry, no JSON."),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn fallback_rolls_over_only_when_the_user_built_a_chain_and_marks_attention() {
    let failing = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({ "error": { "message": "bad key" } })),
        )
        .mount(&failing)
        .await;
    let working = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(chat(GOOD))
        .mount(&working)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let first = record("prov_a", "openai_compatible", &format!("{}/v1", failing.uri()));
    let second = record("prov_b", "local", &format!("{}/v1", working.uri()));
    let vault = MemoryVault::default();
    vault.set(&vault::ai_key("prov_a"), &Secret::new("sk-test")).unwrap();
    let client = AiClient::new().unwrap();
    let router = Router { paths: &paths, vault: &vault, client: &client };
    let ask = Ask { system: "s", user: "u", json_schema: None, max_tokens: 100, work: None };

    let without = ProvidersFile {
        schema: 1,
        providers: vec![first.clone(), second.clone()],
        fallback: vec![],
    };
    records::save(&paths, &without).unwrap();
    let err = router
        .generate(&without, &first, &ask, &CancellationToken::new(), &|_| {})
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Provider(ref e) if e.code == ErrorCode::Auth));
    assert_eq!(
        records::load(&paths).unwrap().providers[0].needs_attention.as_deref(),
        Some("bad key")
    );

    let with = ProvidersFile { fallback: vec!["prov_a".into(), "prov_b".into()], ..without };
    let attempts: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = attempts.clone();
    let answer = router
        .generate(&with, &first, &ask, &CancellationToken::new(), &move |event| {
            if let RouteEvent::Attempt(p) = event {
                log.lock().unwrap().push(p.id);
            }
        })
        .await
        .unwrap();
    assert_eq!(answer.provider.id, "prov_b");
    assert_eq!(answer.fell_back_from.as_deref(), Some("prov_a"));
    assert_eq!(*attempts.lock().unwrap(), ["prov_a", "prov_b"]);
}

#[tokio::test]
async fn a_refusal_never_rolls_over() {
    let refusing = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{ "index": 0, "message": { "role": "assistant", "content": "" }, "finish_reason": "content_filter" }]
        })))
        .mount(&refusing)
        .await;
    let other = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(chat(GOOD))
        .expect(0)
        .mount(&other)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let first = record("prov_a", "local", &format!("{}/v1", refusing.uri()));
    let second = record("prov_b", "local", &format!("{}/v1", other.uri()));
    let file = ProvidersFile {
        schema: 1,
        providers: vec![first.clone(), second],
        fallback: vec!["prov_b".into()],
    };
    let vault = MemoryVault::default();
    let client = AiClient::new().unwrap();
    let router = Router { paths: &paths, vault: &vault, client: &client };
    let ask = Ask {
        system: "s",
        user: "u",
        json_schema: Some(&schemas::DRAFT_RELEASE),
        max_tokens: 100,
        work: None,
    };
    let err = task::ask_json::<DraftAnswer>(
        &router,
        &file,
        &first,
        &ask,
        &schemas::DRAFT_RELEASE,
        &CancellationToken::new(),
        &|_| {},
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TaskError::Refused { .. }), "{err:?}");
}
