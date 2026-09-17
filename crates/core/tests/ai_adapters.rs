//! Every adapter's request building and response parsing against recorded fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use serde_json::{Value, json};
use svnpush_core::ai::adapters::revoye;
use svnpush_core::ai::provider::{
    Adapter, ErrorCode, FinishReason, GenerateRequest, Message, Method, ParseOutcome, Role, WorkId,
};
use svnpush_core::ai::registry;

fn fixture(path: &str) -> Value {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ai").join(path);
    serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap()
}

fn adapter(kind: &str) -> &'static dyn Adapter {
    registry::get(kind).unwrap()
}

fn user(text: &str) -> Vec<Message> {
    vec![Message { role: Role::User, content: text.into() }]
}

fn request<'a>(messages: &'a [Message], schema: Option<&'a Value>) -> GenerateRequest<'a> {
    GenerateRequest {
        base_url: None,
        api_key: Some("test-key"),
        model: "revoye/auto",
        system: Some("You write release notes."),
        messages,
        max_tokens: Some(2000),
        temperature: None,
        json_schema: schema,
        work: Some(WorkId { slug: "demo", version: "1.2.0", task: "draft_release" }),
    }
}

fn header<'a>(req: &'a svnpush_core::ai::provider::HttpRequest, name: &str) -> Option<&'a str> {
    req.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn complete(outcome: ParseOutcome) -> svnpush_core::ai::provider::GenerateResult {
    match outcome {
        ParseOutcome::Complete(result) => result,
        ParseOutcome::Pending { .. } => panic!("expected a complete result"),
    }
}

// ── Revoye ───────────────────────────────────────────────────────────────

#[test]
fn revoye_submit_is_async_with_a_work_derived_idempotency_key() {
    let messages = user("What changed?");
    let schema = json!({ "type": "object" });
    let req = adapter("revoye").build_request(&request(&messages, Some(&schema))).unwrap();
    assert_eq!(req.method, Method::Post);
    assert_eq!(req.url, "https://api.revoye.com/v1/completions");
    assert_eq!(header(&req, "authorization"), Some("Bearer test-key"));
    let key = header(&req, "idempotency-key").unwrap().to_owned();
    assert!(key.starts_with("svnpush:demo:1.2.0:draft_release:"));
    assert_eq!(key.rsplit(':').next().unwrap().len(), 16);
    let body = req.body.unwrap();
    assert_eq!(body["wait"], false);
    assert_eq!(body["provider"], Value::Null);
    assert_eq!(body["timeout_ms"], 180_000);
    assert_eq!(body["deadline_ms"], 600_000);
    assert_eq!(body["metadata"], json!({ "source": "svnpush", "task": "draft_release" }));
    let prompt = body["prompt"].as_str().unwrap();
    assert!(prompt.starts_with(
        "You write release notes.\n\nWhat changed?\n\nReply with a single valid JSON object"
    ));

    let again = adapter("revoye").build_request(&request(&messages, Some(&schema))).unwrap();
    assert_eq!(header(&again, "idempotency-key"), Some(key.as_str()));
}

#[test]
fn revoye_pins_a_provider_kind_and_rejects_unknown_ones() {
    let messages = user("Hi");
    let mut req = request(&messages, None);
    req.model = "revoye/claude";
    assert_eq!(adapter("revoye").build_request(&req).unwrap().body.unwrap()["provider"], "claude");
    req.model = "gpt-9";
    assert_eq!(adapter("revoye").build_request(&req).unwrap_err().code, ErrorCode::BadRequest);
}

#[test]
fn revoye_flattens_multi_turn_conversations_and_limits_size() {
    let messages = vec![
        Message { role: Role::User, content: "one".into() },
        Message { role: Role::Assistant, content: "two".into() },
        Message { role: Role::User, content: "three".into() },
    ];
    let mut req = request(&messages, None);
    req.system = None;
    assert_eq!(revoye::flatten_prompt(&req).unwrap(), "User: one\n\nAssistant: two\n\nUser: three");

    let empty = user("   ");
    let mut req = request(&empty, None);
    req.system = None;
    assert_eq!(revoye::flatten_prompt(&req).unwrap_err().code, ErrorCode::BadRequest);

    let huge = user(&"x".repeat(revoye::MAX_PROMPT_CHARS + 1));
    let mut req = request(&huge, None);
    req.system = None;
    assert!(revoye::flatten_prompt(&req).unwrap_err().message.contains("100000"));
}

#[test]
fn revoye_job_statuses() {
    let revoye = adapter("revoye");
    match revoye.parse_response(&fixture("revoye/queued.json")).unwrap() {
        ParseOutcome::Pending { job_id, status, queue_position } => {
            assert_eq!(
                (job_id.as_str(), status.as_str(), queue_position),
                ("job_01JAY7Q2K8XYZ", "queued", Some(3))
            );
        }
        ParseOutcome::Complete(_) => panic!("queued is pending"),
    }
    assert!(matches!(
        revoye.poll().unwrap().parse_response(&fixture("revoye/dispatched.json")).unwrap(),
        ParseOutcome::Pending { queue_position: None, .. }
    ));
    let done = complete(revoye.parse_response(&fixture("revoye/succeeded.json")).unwrap());
    assert_eq!(done.text, r#"{"version": "1.2.0"}"#);
    assert_eq!((done.tokens_in, done.tokens_out), (None, None));

    let failed = revoye.parse_response(&fixture("revoye/failed.json")).unwrap_err();
    assert_eq!(failed.code, ErrorCode::Server);
    assert!(failed.message.contains("3 attempt"));
    for status in ["expired", "cancelled"] {
        let e = revoye.parse_response(&fixture(&format!("revoye/{status}.json"))).unwrap_err();
        assert_eq!(e.code, ErrorCode::Server, "{status}");
    }
    let pruned = revoye.parse_response(&fixture("revoye/pruned.json")).unwrap_err();
    assert_eq!(pruned.code, ErrorCode::Server);
    assert!(pruned.message.contains("Run it again"));
}

#[test]
fn revoye_every_error_code() {
    let revoye = adapter("revoye");
    let cases: [(&str, u16, ErrorCode); 16] = [
        ("INVALID_REQUEST", 400, ErrorCode::BadRequest),
        ("UNAUTHORIZED", 401, ErrorCode::Auth),
        ("FORBIDDEN_scope", 403, ErrorCode::Auth),
        ("FORBIDDEN_limit", 403, ErrorCode::RateLimit),
        ("NOT_FOUND", 404, ErrorCode::Server),
        ("CONFLICT", 409, ErrorCode::BadRequest),
        ("PAYLOAD_TOO_LARGE", 413, ErrorCode::BadRequest),
        ("RATE_LIMITED", 429, ErrorCode::RateLimit),
        ("JOB_CANCELLED", 499, ErrorCode::Server),
        ("INTERNAL_ERROR", 500, ErrorCode::Server),
        ("JOB_FAILED", 502, ErrorCode::Server),
        ("NO_DEVICE_ONLINE", 503, ErrorCode::Server),
        ("NO_AGENT_AVAILABLE", 503, ErrorCode::Server),
        ("PROVIDER_RATE_LIMITED", 503, ErrorCode::RateLimit),
        ("JOB_TIMEOUT", 504, ErrorCode::Server),
        ("UNKNOWN", 418, ErrorCode::BadRequest),
    ];
    for (file, status, code) in cases {
        let e = revoye.parse_error(status, &fixture(&format!("revoye/errors/{file}.json")));
        assert_eq!(e.code, code, "{file}");
        assert_eq!(e.status, status, "{file}");
        assert!(!e.message.contains("must never be parsed"), "{file} used the human message");
    }
    let limited = revoye.parse_error(429, &fixture("revoye/errors/RATE_LIMITED.json"));
    assert_eq!(limited.retry_after, Some(12));
    let scope = revoye.parse_error(403, &fixture("revoye/errors/FORBIDDEN_scope.json"));
    assert!(scope.message.contains("completions:write"));
    let no_device = revoye.parse_error(503, &fixture("revoye/errors/NO_DEVICE_ONLINE.json"));
    assert!(no_device.message.contains("Revoye Desk"));
    assert_eq!(revoye.parse_error(404, &json!({})).code, ErrorCode::Server);
}

#[test]
fn revoye_models_status_poll_and_cancel() {
    let revoye = adapter("revoye");
    let lister = revoye.list_models().unwrap();
    assert_eq!(lister.build_request(None, Some("k")).url, "https://api.revoye.com/v1/models");
    let list = lister.parse_response(&fixture("revoye/models.json"));
    assert_eq!(list.models, ["revoye/auto", "chatgpt", "deepseek"]);
    let empty = lister.parse_response(&fixture("revoye/models_empty.json"));
    assert!(empty.models.is_empty());
    assert_eq!(empty.note.as_deref(), Some("No providers are enabled on this account."));

    let fleet = revoye.fleet_status().unwrap().parse_response(&fixture("revoye/status.json"));
    assert_eq!((fleet.devices_online, fleet.agents_idle, fleet.queue_depth), (1, 3, 4));
    assert_eq!(fleet.providers[0].agents_idle, 2);

    let poll = revoye.poll().unwrap();
    assert_eq!((poll.interval_ms(), poll.max_ms()), (4_000, 600_000));
    assert_eq!(poll.build_request("k", "job_1").url, "https://api.revoye.com/v1/completions/job_1");
    let cancel = poll.build_cancel("k", "job/../x");
    assert_eq!(cancel.method, Method::Delete);
    assert_eq!(cancel.url, "https://api.revoye.com/v1/completions/job%2F..%2Fx");
}

// ── Gemini ───────────────────────────────────────────────────────────────

#[test]
fn gemini_request_and_responses() {
    let gemini = adapter("gemini");
    let messages = user("What changed?");
    let schema = json!({ "type": "object" });
    let mut req = request(&messages, Some(&schema));
    req.model = "gemini-3.8-flash";
    let http = gemini.build_request(&req).unwrap();
    assert_eq!(
        http.url,
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.8-flash:generateContent"
    );
    assert_eq!(header(&http, "x-goog-api-key"), Some("test-key"));
    let body = http.body.unwrap();
    assert_eq!(body["generationConfig"]["responseMimeType"], "application/json");
    assert_eq!(body["generationConfig"]["responseJsonSchema"], schema);
    assert_eq!(body["systemInstruction"]["parts"][0]["text"], "You write release notes.");
    assert_eq!(body["contents"][0]["role"], "user");

    let ok = complete(gemini.parse_response(&fixture("gemini/success.json")).unwrap());
    assert_eq!(ok.text, r#"{"version": "1.2.0"}"#);
    assert_eq!((ok.tokens_in, ok.tokens_out), (Some(812), Some(64)));
    let cut = complete(gemini.parse_response(&fixture("gemini/max_tokens.json")).unwrap());
    assert_eq!(cut.finish_reason, FinishReason::Length);
    assert_eq!(
        gemini.parse_response(&fixture("gemini/blocked.json")).unwrap_err().code,
        ErrorCode::Safety
    );
    assert_eq!(gemini.parse_error(400, &fixture("gemini/error_key.json")).code, ErrorCode::Auth);
}

// ── Claude ───────────────────────────────────────────────────────────────

#[test]
fn claude_request_and_responses() {
    let claude = adapter("claude");
    let messages = user("What changed?");
    let schema = json!({ "type": "object" });
    let mut req = request(&messages, Some(&schema));
    req.model = "claude-sonnet-5";
    req.temperature = Some(0.2);
    let http = claude.build_request(&req).unwrap();
    assert_eq!(http.url, "https://api.anthropic.com/v1/messages");
    assert_eq!(header(&http, "x-api-key"), Some("test-key"));
    assert_eq!(header(&http, "anthropic-version"), Some("2023-06-01"));
    let body = http.body.unwrap();
    assert_eq!(
        body["output_config"],
        json!({ "format": { "type": "json_schema", "schema": schema } })
    );
    assert!(body.get("temperature").is_none(), "sampling parameters are never sent");
    assert_eq!(body["system"], "You write release notes.");

    let ok = complete(claude.parse_response(&fixture("claude/success.json")).unwrap());
    assert_eq!((ok.tokens_in, ok.tokens_out), (Some(910), Some(71)));
    assert_eq!(
        claude.parse_response(&fixture("claude/refusal.json")).unwrap_err().code,
        ErrorCode::Safety
    );
    let cut = complete(claude.parse_response(&fixture("claude/max_tokens.json")).unwrap());
    assert_eq!(cut.finish_reason, FinishReason::Length);
    assert_eq!(claude.parse_error(529, &fixture("claude/overloaded.json")).code, ErrorCode::Server);
    assert_eq!(claude.parse_error(402, &fixture("claude/billing.json")).code, ErrorCode::Payment);
}

// ── OpenAI family ────────────────────────────────────────────────────────

#[test]
fn openai_family_requests_differ_only_where_they_must() {
    let messages = user("What changed?");
    let schema = json!({ "type": "object" });
    let mut req = request(&messages, Some(&schema));
    req.model = "gpt-6-astra";

    let openai = adapter("openai").build_request(&req).unwrap();
    assert_eq!(openai.url, "https://api.openai.com/v1/chat/completions");
    let body = openai.body.unwrap();
    assert_eq!(body["max_completion_tokens"], 2000);
    assert!(body.get("max_tokens").is_none());
    assert_eq!(body["response_format"], json!({ "type": "json_object" }));
    assert_eq!(
        body["messages"][0],
        json!({ "role": "system", "content": "You write release notes." })
    );

    let deepseek = adapter("deepseek").build_request(&req).unwrap();
    assert_eq!(deepseek.url, "https://api.deepseek.com/v1/chat/completions");
    assert_eq!(deepseek.body.unwrap()["max_tokens"], 2000);

    let mut local_req = req;
    local_req.api_key = None;
    let local = adapter("local").build_request(&local_req).unwrap();
    assert_eq!(local.url, "http://localhost:11434/v1/chat/completions");
    assert!(header(&local, "authorization").is_none());

    assert_eq!(
        adapter("openai_compatible").build_request(&req).unwrap_err().code,
        ErrorCode::BadRequest
    );
    let mut custom = req;
    custom.base_url = Some("https://gateway.example/v1/");
    assert_eq!(
        adapter("openai_compatible").build_request(&custom).unwrap().url,
        "https://gateway.example/v1/chat/completions"
    );
}

#[test]
fn openai_responses_and_errors() {
    let openai = adapter("openai");
    let ok = complete(openai.parse_response(&fixture("openai/success.json")).unwrap());
    assert_eq!(ok.text, r#"{"version": "1.2.0"}"#);
    assert_eq!((ok.tokens_in, ok.tokens_out), (Some(850), Some(60)));
    let cut = complete(openai.parse_response(&fixture("openai/length.json")).unwrap());
    assert_eq!(cut.finish_reason, FinishReason::Length);
    assert_eq!((cut.tokens_in, cut.tokens_out), (None, None));
    let e = openai.parse_error(401, &fixture("openai/error_401.json"));
    assert_eq!(e.code, ErrorCode::Auth);
    assert_eq!(e.message, "Incorrect API key provided.");
}

// ── OpenRouter ───────────────────────────────────────────────────────────

#[test]
fn openrouter_attribution_inline_errors_and_catalogue() {
    let router = adapter("openrouter");
    let messages = user("What changed?");
    let http = router.build_request(&request(&messages, None)).unwrap();
    assert_eq!(http.url, "https://openrouter.ai/api/v1/chat/completions");
    assert_eq!(header(&http, "HTTP-Referer"), Some("https://github.com/FlyToRakib/svnpush"));
    assert_eq!(header(&http, "X-OpenRouter-Title"), Some("SVNpush"));

    assert!(router.parse_response(&fixture("openrouter/success.json")).is_ok());
    let inline = router.parse_response(&fixture("openrouter/inline_error.json")).unwrap_err();
    assert_eq!((inline.code, inline.status), (ErrorCode::Server, 502));
    assert!(inline.message.contains("via Anthropic"));
    assert_eq!(
        router.parse_response(&fixture("openrouter/finish_error.json")).unwrap_err().code,
        ErrorCode::Server
    );
    assert_eq!(
        router.parse_response(&fixture("openrouter/filtered.json")).unwrap_err().code,
        ErrorCode::Safety
    );
    assert_eq!(
        router.parse_error(400, &fixture("openrouter/error_invalid_model.json")).code,
        ErrorCode::ModelNotFound
    );
    let models = router.list_models().unwrap().parse_response(&fixture("openrouter/models.json"));
    assert_eq!(models.models, ["anthropic/claude-sonnet-5", "openai/gpt-6-astra"]);
}
