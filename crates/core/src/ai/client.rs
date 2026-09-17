//! The only place an AI request is sent (plan §9.4): timeouts, the poll
//! loop, cancellation, and one wait on a poll rate limit.

use std::time::{Duration, Instant};

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::error::Coded;

use super::provider::{
    Adapter, ErrorCode, Fleet, GenerateRequest, GenerateResult, HttpRequest, Method, ModelList,
    ParseOutcome, ProviderError,
};

/// Submit, list, status, poll and cancel calls.
pub const QUICK_TIMEOUT: Duration = Duration::from_secs(30);
/// A synchronous model answer can take minutes.
pub const GENERATE_TIMEOUT: Duration = Duration::from_secs(180);

/// A job still running, reported while polling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    /// Job id.
    pub job_id: String,
    /// Provider status.
    pub status: String,
    /// Jobs ahead, when known.
    pub queue_position: Option<u32>,
}

/// Why a generation did not produce a result.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    /// The provider failed.
    #[error(transparent)]
    Provider(#[from] ProviderError),
    /// The developer cancelled.
    #[error("cancelled")]
    Cancelled,
}

impl Coded for ClientError {
    fn code(&self) -> &'static str {
        match self {
            Self::Provider(e) => e.code(),
            Self::Cancelled => "CANCELLED",
        }
    }

    fn fix(&self) -> Option<String> {
        match self {
            Self::Provider(e) => e.fix(),
            Self::Cancelled => None,
        }
    }
}

/// Sends adapter requests.
#[derive(Debug, Clone)]
pub struct AiClient {
    http: reqwest::Client,
    rewrite: Option<(String, String)>,
}

struct Response {
    status: u16,
    json: Value,
    retry_after: Option<u32>,
}

impl AiClient {
    /// A client with the platform's TLS roots.
    pub fn new() -> Result<Self, ProviderError> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("SVNpush/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| {
                ProviderError::new(
                    ErrorCode::Network,
                    "client",
                    format!("Could not start the HTTP client: {e}"),
                )
            })?;
        Ok(Self { http, rewrite: None })
    }

    /// Sends every request whose URL starts with `from` to `to` instead.
    /// Used by tests to point fixed-endpoint adapters at a mock server.
    #[must_use]
    pub fn with_url_rewrite(mut self, from: &str, to: &str) -> Self {
        self.rewrite = Some((from.to_owned(), to.to_owned()));
        self
    }

    async fn send(
        &self,
        req: &HttpRequest,
        timeout: Duration,
        kind: &'static str,
    ) -> Result<Response, ProviderError> {
        let url = match &self.rewrite {
            Some((from, to)) if req.url.starts_with(from.as_str()) => {
                format!("{to}{}", &req.url[from.len()..])
            }
            _ => req.url.clone(),
        };
        let method = match req.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Delete => reqwest::Method::DELETE,
        };
        let mut builder = self.http.request(method, &url).timeout(timeout);
        for (name, value) in &req.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = &req.body {
            builder = builder.body(body.to_string());
        }
        let response = builder.send().await.map_err(|e| {
            let message = if e.is_timeout() {
                "The AI provider did not answer in time.".to_owned()
            } else if e.is_connect() {
                "Could not connect to the AI provider.".to_owned()
            } else {
                "The request to the AI provider failed.".to_owned()
            };
            tracing::warn!(
                provider = kind,
                timeout = e.is_timeout(),
                connect = e.is_connect(),
                "ai request failed"
            );
            ProviderError::new(ErrorCode::Network, kind, message)
        })?;
        let status = response.status().as_u16();
        let request_id =
            response.headers().get("x-request-id").and_then(|v| v.to_str().ok()).map(str::to_owned);
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u32>().ok());
        let text = response.text().await.map_err(|_| {
            ProviderError::new(ErrorCode::Network, kind, "The AI provider's answer was cut off.")
        })?;
        let json = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !(200..300).contains(&status) {
            tracing::warn!(
                provider = kind,
                status,
                request_id = request_id.as_deref(),
                "ai request rejected"
            );
        }
        Ok(Response { status, json, retry_after })
    }

    fn check(adapter: &dyn Adapter, response: Response) -> Result<Value, ProviderError> {
        if (200..300).contains(&response.status) {
            return Ok(response.json);
        }
        let mut error = adapter.parse_error(response.status, &response.json);
        if error.retry_after.is_none() {
            error.retry_after = response.retry_after;
        }
        tracing::warn!(
            provider = adapter.meta().kind,
            code = error.code.as_str(),
            status = error.status,
            "ai error"
        );
        Err(error)
    }

    /// Runs one generation to completion, polling when the adapter is asynchronous.
    pub async fn generate(
        &self,
        adapter: &dyn Adapter,
        request: &GenerateRequest<'_>,
        cancel: &CancellationToken,
        on_pending: &(dyn Fn(&Pending) + Send + Sync),
    ) -> Result<GenerateResult, ClientError> {
        let kind = adapter.meta().kind;
        let http = adapter.build_request(request)?;
        let timeout = if adapter.poll().is_some() { QUICK_TIMEOUT } else { GENERATE_TIMEOUT };
        let response = tokio::select! {
            () = cancel.cancelled() => return Err(ClientError::Cancelled),
            r = self.send(&http, timeout, kind) => r?,
        };
        let json = Self::check(adapter, response)?;
        let (job_id, status, queue_position) = match adapter.parse_response(&json)? {
            ParseOutcome::Complete(result) => return Ok(result),
            ParseOutcome::Pending { job_id, status, queue_position } => {
                (job_id, status, queue_position)
            }
        };
        let Some(poll) = adapter.poll() else {
            return Err(ProviderError::new(
                ErrorCode::Server,
                kind,
                "The provider returned a job it cannot poll.",
            )
            .into());
        };
        let api_key = request.api_key.unwrap_or_default();
        on_pending(&Pending { job_id: job_id.clone(), status, queue_position });

        let started = Instant::now();
        let mut waited_for_rate_limit = false;
        loop {
            let pause = Duration::from_millis(poll.interval_ms());
            tokio::select! {
                () = cancel.cancelled() => {
                    self.cancel_job(adapter, api_key, &job_id).await;
                    return Err(ClientError::Cancelled);
                }
                () = tokio::time::sleep(pause) => {}
            }
            if started.elapsed() > Duration::from_millis(poll.max_ms()) {
                return Err(ProviderError::new(
                    ErrorCode::Server,
                    kind,
                    "The job did not finish in time. It may still complete on the provider's side.",
                )
                .into());
            }
            let poll_request = poll.build_request(api_key, &job_id);
            let response = tokio::select! {
                () = cancel.cancelled() => {
                    self.cancel_job(adapter, api_key, &job_id).await;
                    return Err(ClientError::Cancelled);
                }
                r = self.send(&poll_request, QUICK_TIMEOUT, kind) => r?,
            };
            let json = match Self::check(adapter, response) {
                Ok(json) => json,
                Err(e) if e.code == ErrorCode::RateLimit && !waited_for_rate_limit => {
                    waited_for_rate_limit = true;
                    let seconds = u64::from(e.retry_after.unwrap_or(5));
                    tokio::select! {
                        () = cancel.cancelled() => {
                            self.cancel_job(adapter, api_key, &job_id).await;
                            return Err(ClientError::Cancelled);
                        }
                        () = tokio::time::sleep(Duration::from_secs(seconds)) => continue,
                    }
                }
                Err(e) => return Err(e.into()),
            };
            match poll.parse_response(&json)? {
                ParseOutcome::Complete(result) => return Ok(result),
                ParseOutcome::Pending { job_id: id, status, queue_position } => {
                    on_pending(&Pending { job_id: id, status, queue_position });
                }
            }
        }
    }

    /// Aborting the HTTP request does not stop a server-side job; this does. Best effort.
    async fn cancel_job(&self, adapter: &dyn Adapter, api_key: &str, job_id: &str) {
        if let Some(poll) = adapter.poll() {
            let request = poll.build_cancel(api_key, job_id);
            if self.send(&request, QUICK_TIMEOUT, adapter.meta().kind).await.is_err() {
                tracing::warn!(provider = adapter.meta().kind, "could not cancel the job");
            }
        }
    }

    /// The models an account can use.
    pub async fn list_models(
        &self,
        adapter: &dyn Adapter,
        base_url: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<ModelList, ProviderError> {
        let kind = adapter.meta().kind;
        let lister = adapter.list_models().ok_or_else(|| {
            ProviderError::new(ErrorCode::BadRequest, kind, "This provider has no model list.")
        })?;
        let response =
            self.send(&lister.build_request(base_url, api_key), QUICK_TIMEOUT, kind).await?;
        Ok(lister.parse_response(&Self::check(adapter, response)?))
    }

    /// A capacity snapshot.
    pub async fn fleet_status(
        &self,
        adapter: &dyn Adapter,
        api_key: &str,
    ) -> Result<Fleet, ProviderError> {
        let kind = adapter.meta().kind;
        let fleet = adapter.fleet_status().ok_or_else(|| {
            ProviderError::new(ErrorCode::BadRequest, kind, "This provider has no fleet status.")
        })?;
        let response = self.send(&fleet.build_request(api_key), QUICK_TIMEOUT, kind).await?;
        Ok(fleet.parse_response(&Self::check(adapter, response)?))
    }
}
