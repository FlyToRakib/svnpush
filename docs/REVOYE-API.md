# Revoye Public API — Node.js integration skill

**What Revoye is:** an HTTP API that takes a prompt and returns an answer produced by an AI account
*you* are already signed into, driven by a real browser on your own machine. You integrate it the
way you integrate any AI provider: one base URL, one bearer key, a few endpoints.

**What is different from a model API, and shapes every design decision below:**

1. **A job takes 20–90 seconds.** A browser is typing the prompt and a model is generating into a
   page. Set HTTP timeouts to 300 s+, or do not block at all.
2. **`wait: true` is a long poll, not synchronous execution.** The job is durable the moment it is
   accepted. A dropped connection loses nothing — the result stays at `GET /v1/completions/{id}`
   and is delivered to `callback_url` if you gave one.
3. **The user's machine must be on.** If no paired device is connected, work queues.
4. **There are no token counts.** `usage` is `null` everywhere. Never do arithmetic on it.

Scope: this file covers **only** the public API (`/v1/*`). The dashboard API (`/api/*`, session +
CSRF) and the device gateway (`/v1/pair`, `/v1/challenge`, `/v1/agent`) are out of scope and are
not reachable with an API key.

---

## 1. Quick start

Requires **Node 18+** (global `fetch`, `AbortSignal.timeout`). Node 20+ recommended. No
dependencies.

```sh
# nothing to install — global fetch is enough
node --version   # v18.0.0 or later
```

```sh
# .env
REVOYE_BASE_URL=https://api.revoye.com
REVOYE_API_KEY=revoye_sk_live_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

```js
// quickstart.mjs
const BASE = process.env.REVOYE_BASE_URL;
const KEY = process.env.REVOYE_API_KEY;

const res = await fetch(`${BASE}/v1/completions`, {
  method: 'POST',
  headers: {
    Authorization: `Bearer ${KEY}`,
    'Content-Type': 'application/json',
    'Idempotency-Key': crypto.randomUUID(),
  },
  body: JSON.stringify({
    prompt: 'Summarise the CAP theorem in three sentences.',
    provider: 'chatgpt',   // or omit for "any enabled provider"
    wait: true,
  }),
  // A browser is doing the work. A 30 s default would abandon a healthy job.
  signal: AbortSignal.timeout(300_000),
});

const body = await res.json();

if (!res.ok) {
  // `code` is the contract; `message` is for humans and may change in any release.
  throw new Error(`${body.error.code}: ${body.error.message}`);
}

console.log(body.status);    // "succeeded"
console.log(body.response);  // the answer text
console.log(body.id);        // "job_01JAY7Q2K8XYZ" — keep it; it is how you fetch or cancel
```

Before any of this works, the account behind the key needs: a verified email, **Revoye Desk**
paired, the **Revoye extension** installed with at least one agent, and an API key.
`GET /v1/status` tells you whether it is ready.

---

## 2. Authentication

One mechanism, no OAuth, no sessions, no request signing.

```http
Authorization: Bearer revoye_sk_live_3xQ8vP2mK9wR7tY4nL6jH1sD5fG0aZbC
```

### Key format

```text
revoye_sk_live_<32 bytes, base62>
└──┬──┘└┬┘└─┬┘
   │    │   └─ environment: live | test   (revoye_sk_live_… / revoye_sk_test_…)
   │    └───── key type: sk (secret key)
   └────────── vendor prefix — registered with secret scanners, so a leaked key is detectable
```

- Created in the Revoye dashboard. **Shown exactly once** — Revoye stores only a hash. Lost key →
  create a new one.
- Max 25 active keys per account.
- Revocation is **immediate**, not after a cache TTL.
- Optional expiry; checked on every request.

### Scopes

| Scope | Grants |
| --- | --- |
| `completions:write` | `POST /v1/completions`, `POST /v1/chat/completions`, `DELETE /v1/completions/{id}` |
| `completions:read` | `GET /v1/completions/{id}` |
| `status:read` | `GET /v1/status`, `GET /v1/models` |

New keys get all three by default. Narrow them: a submit-only worker needs `completions:write`; a
monitoring probe needs `status:read` alone.

**An API key can never** create another key, pair or revoke a device, change providers/agents/rate
limits, or read the audit log. Those are session-only dashboard operations. A leaked key cannot
escalate into account control — its blast radius is the user's agent time until the key is revoked.

### Configuration and failures

```js
// Fail at boot, not inside a job queue three hours from now.
const BASE = (process.env.REVOYE_BASE_URL ?? '').replace(/\/+$/, '');
const KEY = process.env.REVOYE_API_KEY ?? '';
if (!BASE || !KEY) throw new Error('REVOYE_BASE_URL and REVOYE_API_KEY are required');
```

| Result | Meaning | Fix |
| --- | --- | --- |
| `401 UNAUTHORIZED` | Missing, malformed, expired or revoked key | Check the header format and the key itself |
| `403 FORBIDDEN` | Valid key, missing scope (`details.required_scope` names it) | Create a key with the scope |

`401` = the credential is wrong. `403` = the credential is right and the permission is not. Never
retry either.

**Never hardcode a key.** Environment variables or a secret manager only — see §12.

---

## 3. Base API configuration

| | |
| --- | --- |
| Production base URL | `https://api.revoye.com` |
| Public API prefix | `/v1` |
| Dashboard (keys, devices, agents) | `https://revoye.com` |
| Protocol | HTTPS only |
| Request body | JSON, UTF-8, **max 1 MiB** |
| Response body | JSON, UTF-8 |
| Field naming | `snake_case` |
| Timestamps | RFC 3339 with `Z` (e.g. `2026-08-18T09:14:02Z`) |

Put the base URL in an environment variable rather than a constant — a self-hosted or staging
deployment uses a different host.

### Required headers

| Header | When | Value |
| --- | --- | --- |
| `Authorization` | Every request | `Bearer <api key>` |
| `Content-Type` | Every request with a body | `application/json` |
| `Idempotency-Key` | Recommended on `POST` | Opaque string, ≤ 255 chars |

### Response headers

| Header | Meaning |
| --- | --- |
| `X-Request-Id` | On every response. **Log it on failures** — quoting it makes the whole trace recoverable in support |
| `Retry-After` | On `429` only. Seconds to wait |

### Versioning

`/v1` is a stable contract. Additive changes (a new optional field, a new error code) may ship at
any time — **parse defensively and ignore unknown fields**. Anything that would break a working
integration ships as `/v2` alongside it.

### CORS

`/v1/*` responds with `Access-Control-Allow-Origin: *` and **no** credentials; allowed headers are
`content-type`, `authorization`, `idempotency-key`. This exists for tooling, **not** as permission
to put a key in a browser bundle. Always call Revoye from your server.

---

## 4. Complete public API reference

Six endpoints. That is the whole public surface.

| Method | Path | Scope | Purpose |
| --- | --- | --- | --- |
| `POST` | `/v1/completions` | `completions:write` | Submit a prompt (native endpoint) |
| `GET` | `/v1/completions/{id}` | `completions:read` | Fetch a job |
| `DELETE` | `/v1/completions/{id}` | `completions:write` | Cancel a job |
| `POST` | `/v1/chat/completions` | `completions:write` | OpenAI-shaped submission (async only) |
| `GET` | `/v1/models` | `status:read` | Enabled providers, OpenAI list shape |
| `GET` | `/v1/status` | `status:read` | Fleet + queue snapshot |

Shared vocabulary:

```text
provider kinds   chatgpt | deepseek | perplexity | gemini | claude | qwen
job statuses     queued | dispatched | succeeded | failed | cancelled | expired
```

---

### 4.1 `POST /v1/completions`

Submit a prompt. Everything Revoye can do is expressible here — this is the endpoint to build on.

**Auth:** `completions:write`. **Headers:** `Authorization`, `Content-Type: application/json`,
optional `Idempotency-Key`.

#### Request body

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `prompt` | string, 1–100 000 chars | **required** | The text typed into the provider |
| `provider` | provider kind or `null` | `null` | `null` = any enabled provider, chosen by the account's rotation strategy |
| `agent_id` | string, 1–64 chars, or `null` | `null` | Pin to one agent. `NO_AGENT_AVAILABLE` if it is not eligible. Agent ids come from the dashboard, not from this API |
| `wait` | boolean | `true` | `true` holds the HTTP request open until the job settles |
| `timeout_ms` | int 5 000–600 000 | 180 000 | Per **attempt** |
| `deadline_ms` | int 10 000–3 600 000 | 900 000 | For the whole job, across attempts. Also caps how long `wait: true` holds the connection |
| `priority` | int −10…10 | `0` | Higher dispatches first **within your own queue** |
| `mode` | `"new"` \| `"continue"` | `"new"` | `"continue"` requires `conversation_ref` |
| `conversation_ref` | string 1–2048, or `null` | `null` | Opaque handle returned by a previous completion |
| `callback_url` | **https** URL ≤ 2048 chars, or `null` | `null` | Webhook on completion. See §10.5 |
| `metadata` | object, ≤ 4 KiB serialised | `{}` | Echoed back verbatim on the completion **and** on the webhook. Revoye never reads it |

Choosing a provider is a **constraint, not a preference** — a `chatgpt` request is never silently
answered by DeepSeek.

#### Example request

```js
const res = await fetch(`${BASE}/v1/completions`, {
  method: 'POST',
  headers: {
    Authorization: `Bearer ${KEY}`,
    'Content-Type': 'application/json',
    'Idempotency-Key': '4f1c9d2e-8b3a-4c1d-9e2f-7a6b5c4d3e2f',
  },
  body: JSON.stringify({
    prompt: 'Summarise the CAP theorem in three sentences.',
    provider: 'chatgpt',
    wait: true,
    timeout_ms: 180_000,
    deadline_ms: 900_000,
    metadata: { trace: 'abc' },
  }),
  signal: AbortSignal.timeout(300_000),
});
```

#### Response — `wait: true`, settled successfully (`200`)

```json
{
  "id": "job_01JAY7Q2K8XYZ",
  "status": "succeeded",
  "response": "The CAP theorem states that…",
  "provider": "chatgpt",
  "agent_id": "agt_01JAY7…",
  "agent_name": null,
  "conversation_ref": "https://chatgpt.com/c/6f0a…",
  "attempts": 1,
  "queue_ms": 240,
  "run_ms": 18432,
  "created_at": "2026-08-18T09:14:02Z",
  "finished_at": "2026-08-18T09:14:21Z",
  "content_pruned_at": null,
  "metadata": { "trace": "abc" }
}
```

| Field | Means |
| --- | --- |
| `id` | Job id. Use it for `GET`/`DELETE` and to key your own records |
| `status` | One of the six job statuses |
| `response` | The answer text. `null` until the job succeeds — and `null` afterwards if content was pruned |
| `provider` | The provider **constraint** you sent (`null` if you sent none) |
| `agent_id` | The agent that took it, or `null` if never dispatched |
| `agent_name` | Always `null` on the public API |
| `conversation_ref` | Opaque handle to the thread in the provider's own UI. Pass it back with `mode: "continue"` |
| `attempts` | How many agents this was dispatched to. `> 1` means something timed out or failed |
| `queue_ms` | Wait before the **first** dispatch. Set once, so a retried job still reports its original wait |
| `run_ms` | Time spent executing |
| `content_pruned_at` | When prompt and response were removed under the account's retention setting. **Non-null with `status: "succeeded"` and `response: null` means the text expired, not that something broke** |
| `metadata` | Your object, byte for byte |
| `queue_position` | Present **only** when `status` is `queued`. It is the account's current queue depth — an upper bound on your place in line, not an exact index |

#### Response — `wait: false` (`202`)

```json
{
  "id": "job_01JAY7Q2K8XYZ",
  "status": "queued",
  "response": null,
  "provider": "chatgpt",
  "agent_id": null,
  "agent_name": null,
  "conversation_ref": null,
  "attempts": 0,
  "queue_ms": null,
  "run_ms": null,
  "created_at": "2026-08-18T09:14:02Z",
  "finished_at": null,
  "content_pruned_at": null,
  "metadata": {}
}
```

`200` instead of `202` on this path means the `Idempotency-Key` matched an existing job — no new
work was created.

#### Blocking semantics (`wait: true`)

- The connection is held for at most `min(deadline_ms, 600000)` ms — **10 minutes is the hard
  ceiling**, whatever you ask for.
- On expiry: `504 JOB_TIMEOUT` with `details.job_id` and `details.status`, **and the job keeps
  running.** The timeout is your patience, not the job's.
- If your socket closes, Revoye stops waiting but does not touch the job.
- If no device is online, the request **waits** rather than failing. `NO_DEVICE_ONLINE` is returned
  immediately only when `wait: false` **and** no `callback_url` was given — because then there
  would be nobody to tell.
- A terminal failure returns the last attempt's error: `502 JOB_FAILED`, `504` for `expired`,
  `499 JOB_CANCELLED` for `cancelled` — each with `details.job_id`, `details.status`,
  `details.attempts`.

#### Idempotency

Send `Idempotency-Key` (opaque string, ≤ 255 chars, one per **logical operation** — not per HTTP
attempt).

| Situation | Result |
| --- | --- |
| Same key, same body | The **original job** is returned in whatever state it is in (`200`) |
| Same key, different body | `409 CONFLICT` with `details.idempotency_key` |
| No key | Every request creates a new job |

Keys are scoped per account and retained for 24 hours. "Same body" is computed over the fields that
change the work — `prompt`, `provider`, `agent_id`, `mode`, `conversation_ref`, `priority`,
`timeout_ms`, `deadline_ms`, `callback_url`. **`metadata` is excluded**, so retrying with a
different trace id is still the same request.

Derive the key from your own work's natural id (a row id, a message id) where you have one — then a
process that crashes and restarts recomputes the same key instead of remembering one.

#### Errors

| Code | HTTP | Cause |
| --- | --- | --- |
| `INVALID_REQUEST` | 400 | Validation failed. `details.field` names the offender. Also `mode: "continue"` without `conversation_ref` |
| `UNAUTHORIZED` | 401 | Bad or revoked key |
| `FORBIDDEN` | 403 | Missing `completions:write`, **or** the account is at its 1 000-queued-job ceiling (`details.limit`) |
| `CONFLICT` | 409 | Idempotency key reused with a different body |
| `PAYLOAD_TOO_LARGE` | 413 | Prompt over 100 000 chars (`details.limit`), or body over 1 MiB |
| `RATE_LIMITED` | 429 | Ingress limit. Honour `Retry-After` |
| `NO_DEVICE_ONLINE` | 503 | Only when `wait: false` and no `callback_url` |
| `NO_AGENT_AVAILABLE` | 503 | Device connected, nothing eligible |
| `PROVIDER_RATE_LIMITED` | 503 | The account's own hourly cap |
| `JOB_TIMEOUT` | 504 | Held as long as asked; the job continues |
| `JOB_FAILED` | 502 | Attempts exhausted. Read `details.attempts` |
| `JOB_CANCELLED` | 499 | Cancelled by you or from the dashboard |

#### Notes and common use cases

- **Interactive script / CLI:** `wait: true` with a 300 s+ client timeout.
- **Background worker / batch:** `wait: false` + `callback_url`, or `wait: false` + polling.
- **Correlation:** put your own ids in `metadata`; they come back on the completion *and* the
  webhook, so you can route a result an hour later with no database lookup.

---

### 4.2 `GET /v1/completions/{id}`

Fetch a job. **Auth:** `completions:read`.

```js
const res = await fetch(`${BASE}/v1/completions/${jobId}`, {
  headers: { Authorization: `Bearer ${KEY}` },
  signal: AbortSignal.timeout(30_000),
});
const job = await res.json();
```

- Returns the same object as `POST /v1/completions`, always with HTTP `200` — including for
  non-terminal jobs, whose real state is in `status`.
- A `queued` job also carries `queue_position`.
- A missing job and someone else's job both return `404 NOT_FOUND`. This is deliberate: a `403`
  would confirm the job exists.

> **`?wait=true` on this endpoint is not implemented.** Some Revoye documentation mentions it; the
> current API ignores the query parameter and returns immediately. Poll, or use `callback_url`.

**Errors:** `401`, `403` (missing `completions:read`), `404`, `429`.

**Use cases:** polling after `wait: false`; re-reading a job after a `504 JOB_TIMEOUT`; recovering a
result whose HTTP response you lost.

---

### 4.3 `DELETE /v1/completions/{id}`

Cancel a job. **Auth:** `completions:write`.

```js
const res = await fetch(`${BASE}/v1/completions/${jobId}`, {
  method: 'DELETE',
  headers: { Authorization: `Bearer ${KEY}` },
  signal: AbortSignal.timeout(30_000),
});
const job = await res.json();   // 200, the job object
```

- A `queued` job is cancelled immediately.
- A `dispatched` job is marked cancelled and a cancel is relayed to the agent **best-effort** — the
  browser may already be mid-generation and the extension cannot always stop it.
- Cancelling a job that has already finished is a **no-op, not an error** (`200`).

**Errors:** `401`, `403`, `404`, `429`.

---

### 4.4 `POST /v1/chat/completions` (OpenAI compatibility)

A thin translation layer with no logic of its own. **Auth:** `completions:write`.

#### Request body

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `model` | string 1–128 | **required** | `revoye/auto`, or `revoye/<provider kind>`. A bare provider kind (`chatgpt`) is also accepted |
| `messages` | array, 1–256 items | **required** | `{ role: "system" \| "user" \| "assistant", content: string }`; each `content` ≤ 100 000 chars |
| `stream` | boolean | `false` | `true` returns `400 INVALID_REQUEST` with `details.field: "stream"` |

| `model` | Routes to |
| --- | --- |
| `revoye/auto` | Any enabled provider, per the account's rotation strategy |
| `revoye/chatgpt`, `revoye/claude`, `revoye/gemini`, `revoye/deepseek`, `revoye/qwen`, `revoye/perplexity` | That provider |

#### Example request

```js
const res = await fetch(`${BASE}/v1/chat/completions`, {
  method: 'POST',
  headers: {
    Authorization: `Bearer ${KEY}`,
    'Content-Type': 'application/json',
    'Idempotency-Key': crypto.randomUUID(),
  },
  body: JSON.stringify({
    model: 'revoye/chatgpt',
    messages: [
      { role: 'system', content: 'You are terse.' },
      { role: 'user', content: 'Summarise the CAP theorem.' },
    ],
  }),
  signal: AbortSignal.timeout(60_000),
});
```

#### Example response (`202`)

```json
{
  "id": "job_01JAY7Q2K8XYZ",
  "object": "chat.completion",
  "created": 1755561242,
  "model": "revoye/chatgpt",
  "choices": [
    { "index": 0, "message": { "role": "assistant", "content": null }, "finish_reason": null }
  ],
  "usage": null,
  "_revoye": { "job_id": "job_01JAY7Q2K8XYZ", "status": "queued" }
}
```

> ### ⚠ This endpoint never waits
>
> It **always** returns `202` with `content: null`, `finish_reason: null` and
> `_revoye.status: "queued"`. It does not block, has no `wait` flag, and cannot take a
> `callback_url`. To get the answer you must poll `GET /v1/completions/{_revoye.job_id}` until
> `status` is terminal.
>
> **Consequence: a stock OpenAI SDK pointed at Revoye will hand you an empty message, not an
> answer.** For any new Node code, use `POST /v1/completions` with `wait: true` or a webhook. Reach
> for this endpoint only when an existing OpenAI-shaped client must keep compiling, and add the
> polling step yourself.

#### Three documented differences

1. **`messages` are flattened into one prompt.** `system` messages are prepended, then each
   remaining turn as `User: …` / `Assistant: …`, joined by blank lines. This is not a real
   conversation API — the provider's own UI holds the thread. To continue a real thread use
   `/v1/completions` with `conversation_ref` + `mode: "continue"`.
2. **`stream: true` → `400`.** The agent reads a *finished* answer off a page; there is nothing
   partial to forward.
3. **`usage` is `null`.** There are no token counts. Guard any cost/budget arithmetic.

#### Errors

Same envelope and codes as `/v1/completions`, plus:

| Code | HTTP | Cause |
| --- | --- | --- |
| `INVALID_REQUEST` | 400 | `stream: true`; unknown `model`; empty or oversized `messages` |
| `PAYLOAD_TOO_LARGE` | 413 | The **flattened** messages exceed 100 000 chars (`details.chars`, `details.limit`) |

---

### 4.5 `GET /v1/models`

Lists the providers **enabled on the calling account**, in OpenAI's list shape so model pickers work
unchanged. **Auth:** `status:read`. No parameters — the key names the account.

```js
const res = await fetch(`${BASE}/v1/models`, {
  headers: { Authorization: `Bearer ${KEY}` },
  signal: AbortSignal.timeout(30_000),
});
const { data } = await res.json();
```

```json
{
  "object": "list",
  "data": [
    { "id": "revoye/auto", "object": "model", "created": 1755500000, "owned_by": "revoye" },
    { "id": "chatgpt",     "object": "model", "created": 1755500000, "owned_by": "revoye" },
    { "id": "deepseek",    "object": "model", "created": 1755500100, "owned_by": "revoye" }
  ],
  "_revoye": { "user_id": "usr_01JAY7…" }
}
```

- `revoye/auto` heads the list whenever at least one provider is enabled.
- Every other `id` is the **provider kind** — the same vocabulary `provider` uses on
  `/v1/completions`. (Prefix it with `revoye/` when sending it as `model` to
  `/v1/chat/completions`; both forms are accepted there.)
- **Disabled providers are omitted.** This describes what the account can use, not what Revoye
  supports.
- When the list is empty, `_revoye.note` says why — no providers configured, or all disabled. This
  lets you tell an empty fleet from a broken endpoint.
- This reflects **configuration, not availability.** A provider can appear here while every one of
  its agents is busy or offline. `GET /v1/status` is the endpoint that knows.

**Errors:** `401`, `403` (missing `status:read`), `429`.

---

### 4.6 `GET /v1/status`

The operational snapshot: can anything answer right now? **Auth:** `status:read`. No parameters.

```js
const res = await fetch(`${BASE}/v1/status`, {
  headers: { Authorization: `Bearer ${KEY}` },
  signal: AbortSignal.timeout(30_000),
});
const status = await res.json();
```

```json
{
  "devices": { "total": 2, "online": 1 },
  "agents": { "total": 7, "idle": 3, "busy": 2, "error": 0, "offline": 2 },
  "providers": [
    { "kind": "chatgpt",  "enabled": true, "agents_idle": 2, "rate_limit_per_hour": 60,   "used_this_hour": 14 },
    { "kind": "deepseek", "enabled": true, "agents_idle": 1, "rate_limit_per_hour": null, "used_this_hour": 3 }
  ],
  "queue": { "depth": 4, "oldest_queued_at": "2026-08-18T09:12:00Z" }
}
```

| Field | Means |
| --- | --- |
| `devices.total` | **Active** devices. A revoked device has left the fleet and cannot rejoin without pairing again |
| `devices.online` | Devices connected right now |
| `agents.{total,idle,busy,error,offline}` | Every agent's state. `error` is separate from `offline` on purpose: offline is waiting for a browser, error is waiting for a person |
| `providers[].enabled` | Whether the provider is switched on at all |
| `providers[].agents_idle` | **Narrower than `agents.idle`** — only agents the router could actually dispatch to. An agent switched off in the dashboard or the extension is excluded here while still counted in `agents.*` |
| `providers[].rate_limit_per_hour` | The account's own configured cap. `null` = no cap |
| `providers[].used_this_hour` | Consumption against that cap |
| `queue.depth` | Jobs waiting |
| `queue.oldest_queued_at` | RFC 3339 or `null`. The best single signal that the fleet is undersized |

**Notes**

- `agents.idle: 0` is not an error — it describes the fleet at this instant.
- **Not a lock.** The snapshot can be stale by the time you act on it. Handle
  `NO_AGENT_AVAILABLE` anyway.
- **Not for a tight loop.** Polling per request burns your read budget. Once a minute is plenty.

**Errors:** `401`, `403` (missing `status:read`), `429`.

**Use cases:** pre-flight check before a batch; a capacity dashboard; deciding between `wait: true`
and queueing with a webhook.

---

## 5. Limits reference

| | |
| --- | --- |
| Prompt | 1–100 000 characters |
| `messages` on `/v1/chat/completions` | 1–256 messages, each ≤ 100 000 chars, flattening to ≤ 100 000 chars |
| Request body | 1 MiB |
| `metadata` | 4 KiB serialised |
| Queued jobs per account | 1 000 |
| Per-attempt timeout (`timeout_ms`) | 5 000–600 000 ms (default 180 000) |
| Whole-job deadline (`deadline_ms`) | 10 000–3 600 000 ms (default 900 000) |
| Max connection hold on `wait: true` | 600 000 ms |
| `priority` | −10…10 |
| `Idempotency-Key` | ≤ 255 characters, retained 24 h |
| `callback_url` | https only, ≤ 2048 characters |
| Ingress: writes | 600 / minute **per API key** |
| Ingress: reads | 1 200 / minute **per API key** |
| Active API keys per account | 25 |

---

## 6. Reusable Revoye client

Drop this in as `src/services/revoye.js`. It centralises the base URL, auth, timeouts, response
parsing, typed errors and retries, so application code only calls methods.

```js
// src/services/revoye.js
import { setTimeout as sleep } from 'node:timers/promises';

const BASE = (process.env.REVOYE_BASE_URL ?? '').replace(/\/+$/, '');
const KEY = process.env.REVOYE_API_KEY ?? '';

// Fail at boot, not inside a worker three hours from now.
if (!BASE || !KEY) throw new Error('REVOYE_BASE_URL and REVOYE_API_KEY are required');

/** Long enough for a browser to type a prompt and a model to answer, plus headroom. */
const WAIT_TIMEOUT_MS = 330_000;
const QUICK_TIMEOUT_MS = 30_000;

/** Codes where retrying the same request can succeed. Never includes 4xx you must fix. */
const RETRYABLE_CODES = new Set([
  'RATE_LIMITED',
  'NO_DEVICE_ONLINE',
  'NO_AGENT_AVAILABLE',
  'PROVIDER_RATE_LIMITED',
  'JOB_TIMEOUT',
  'INTERNAL_ERROR',
]);

export class RevoyeError extends Error {
  constructor({ code, message, status, requestId, details, retryAfter }) {
    super(message ?? code ?? 'Revoye request failed');
    this.name = 'RevoyeError';
    this.code = code ?? 'NETWORK_ERROR';
    this.status = status ?? 0;
    this.requestId = requestId ?? null;
    this.details = details ?? null;
    this.retryAfter = retryAfter ?? null;
  }
  get retryable() {
    return RETRYABLE_CODES.has(this.code) || this.code === 'NETWORK_ERROR';
  }
}

async function request(method, path, { body, idempotencyKey, timeoutMs = QUICK_TIMEOUT_MS } = {}) {
  const headers = { Authorization: `Bearer ${KEY}` };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (idempotencyKey) headers['Idempotency-Key'] = idempotencyKey;

  let res;
  try {
    res = await fetch(`${BASE}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch (cause) {
    // DNS, TLS, connection reset, or our own AbortSignal firing.
    throw new RevoyeError({
      code: 'NETWORK_ERROR',
      message:
        cause?.name === 'TimeoutError'
          ? 'Revoye request timed out'
          : String(cause?.message ?? cause),
    });
  }

  const requestId = res.headers.get('x-request-id');
  const payload = await res.json().catch(() => null);

  if (!res.ok) {
    const error = payload?.error ?? {};
    const retryAfter =
      Number(res.headers.get('retry-after')) || error.details?.retry_after_seconds || null;
    throw new RevoyeError({
      code: error.code,
      message: error.message,
      status: res.status,
      requestId: error.request_id ?? requestId,
      details: error.details,
      retryAfter,
    });
  }

  return payload;
}

/** POST with bounded, jittered backoff. Safe only because the idempotency key is fixed. */
async function postWithRetry(path, body, { idempotencyKey, timeoutMs, attempts = 4 } = {}) {
  let delayMs = 2_000;
  for (let attempt = 1; ; attempt += 1) {
    try {
      return await request('POST', path, { body, idempotencyKey, timeoutMs });
    } catch (err) {
      if (!(err instanceof RevoyeError) || !err.retryable || attempt >= attempts) throw err;
      const wait = err.retryAfter ? err.retryAfter * 1000 : delayMs;
      await sleep(wait + Math.random() * 1000);   // jitter, so a fleet does not synchronise
      delayMs = Math.min(delayMs * 2, 60_000);
    }
  }
}

export const revoye = {
  /**
   * Submit a prompt and block until it settles (long poll).
   * @param {string} prompt
   * @param {object} [options] provider, agent_id, timeout_ms, deadline_ms, priority,
   *                           mode, conversation_ref, metadata
   * @param {string} [options.idempotencyKey] one per LOGICAL operation, not per attempt
   */
  async complete(prompt, { idempotencyKey = crypto.randomUUID(), ...options } = {}) {
    return postWithRetry(
      '/v1/completions',
      { prompt, wait: true, ...options },
      { idempotencyKey, timeoutMs: WAIT_TIMEOUT_MS },
    );
  },

  /** Queue a prompt and return immediately. Pass callback_url to be told when it finishes. */
  async submit(prompt, { idempotencyKey = crypto.randomUUID(), ...options } = {}) {
    return postWithRetry('/v1/completions', { prompt, wait: false, ...options }, { idempotencyKey });
  },

  /** Fetch a job by id. */
  async getJob(id) {
    return request('GET', `/v1/completions/${encodeURIComponent(id)}`);
  },

  /** Cancel a job. A terminal job is a no-op, not an error. */
  async cancelJob(id) {
    return request('DELETE', `/v1/completions/${encodeURIComponent(id)}`);
  },

  /** OpenAI-shaped submission. ALWAYS async — poll _revoye.job_id for the answer. */
  async chatCompletion(model, messages, { idempotencyKey = crypto.randomUUID() } = {}) {
    return postWithRetry('/v1/chat/completions', { model, messages }, { idempotencyKey });
  },

  /** Providers enabled on this account, OpenAI list shape. */
  async listModels() {
    return request('GET', '/v1/models');
  },

  /** Fleet and queue snapshot. */
  async status() {
    return request('GET', '/v1/status');
  },

  /** Poll a job to a terminal state. Use only when you cannot receive a webhook. */
  async waitForJob(id, { intervalMs = 5_000, timeoutMs = 900_000 } = {}) {
    const deadline = Date.now() + timeoutMs;
    const TERMINAL = new Set(['succeeded', 'failed', 'cancelled', 'expired']);
    for (;;) {
      const job = await this.getJob(id);
      if (TERMINAL.has(job.status)) return job;
      if (Date.now() >= deadline) {
        throw new RevoyeError({ code: 'JOB_TIMEOUT', message: `Job ${id} still ${job.status}` });
      }
      await sleep(intervalMs);
    }
  },
};
```

### Using it

```js
import { revoye, RevoyeError } from './services/revoye.js';

try {
  const job = await revoye.complete('Summarise the CAP theorem in three sentences.', {
    provider: 'chatgpt',
    metadata: { tenantId: 't_42' },
    idempotencyKey: `summary:${rowId}`,   // derived from your own work
  });
  console.log(job.response);
} catch (err) {
  if (err instanceof RevoyeError) {
    console.error(err.code, err.requestId);   // never log the key or the prompt
  }
  throw err;
}
```

### TypeScript types

```ts
// src/services/revoye.types.ts
export type ProviderKind = 'chatgpt' | 'deepseek' | 'perplexity' | 'gemini' | 'claude' | 'qwen';
export type JobStatus = 'queued' | 'dispatched' | 'succeeded' | 'failed' | 'cancelled' | 'expired';

export interface CompletionJob {
  id: string;
  status: JobStatus;
  response: string | null;
  provider: ProviderKind | null;
  agent_id: string | null;
  agent_name: string | null;
  conversation_ref: string | null;
  attempts: number;
  queue_ms: number | null;
  run_ms: number | null;
  created_at: string;
  finished_at: string | null;
  content_pruned_at: string | null;
  metadata: Record<string, unknown>;
  queue_position?: number;
}

export interface RevoyeErrorBody {
  error: {
    code: string;
    message: string;
    request_id: string;
    details?: Record<string, unknown>;
  };
}

export interface StatusResponse {
  devices: { total: number; online: number };
  agents: { total: number; idle: number; busy: number; error: number; offline: number };
  providers: Array<{
    kind: ProviderKind;
    enabled: boolean;
    agents_idle: number;
    rate_limit_per_hour: number | null;
    used_this_hour: number;
  }>;
  queue: { depth: number; oldest_queued_at: string | null };
}
```

---

## 7. Environment variables

```sh
# .env — never commit this file
REVOYE_BASE_URL=https://api.revoye.com
REVOYE_API_KEY=revoye_sk_live_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

# Only if you receive webhooks (§10.5)
REVOYE_WEBHOOK_SECRET=<the signing secret issued by Revoye>
```

```sh
# .env.example — commit this one, with placeholders only
REVOYE_BASE_URL=https://api.revoye.com
REVOYE_API_KEY=
REVOYE_WEBHOOK_SECRET=
```

```sh
# .gitignore
.env
.env.local
.env.*.local
```

- Load with Node 20.6+ `node --env-file=.env app.js`, or your framework's own loader. In
  production prefer the platform's secret manager (AWS Secrets Manager, GCP Secret Manager, Vault,
  Kubernetes secrets, Docker secrets) over a file.
- **One key per deployment**, not one shared across everything. A key you cannot revoke without
  taking down four systems is a key you will not revoke.
- **Never** use `NEXT_PUBLIC_`, `VITE_`, `REACT_APP_` or any other client-exposed prefix for the
  key. See §12.

---

## 8. Error handling

Every error on every endpoint has the same envelope:

```json
{
  "error": {
    "code": "NO_AGENT_AVAILABLE",
    "message": "No idle agent is available for provider 'chatgpt'.",
    "request_id": "req_01JAY7Q2K8",
    "details": { "provider_kind": "chatgpt" }
  }
}
```

| Field | |
| --- | --- |
| `code` | **The contract. Branch on this.** |
| `message` | For humans; may change in any release. **Never parse it** |
| `request_id` | Also on the `X-Request-Id` response header. Log it on failure |
| `details` | Structured context: `details.field` on validation, `details.attempts` on `JOB_FAILED`, `details.limit` on `PAYLOAD_TOO_LARGE`, `details.retry_after_seconds` on `RATE_LIMITED` |

### Every code

| Code | HTTP | Retry? | What it means |
| --- | --- | --- | --- |
| `INVALID_REQUEST` | 400 | No | Failed validation. `details.field` names the offender |
| `UNAUTHORIZED` | 401 | No | Missing, malformed, expired or revoked key |
| `FORBIDDEN` | 403 | No | Valid key, wrong scope — or the account's queue ceiling |
| `NOT_FOUND` | 404 | No | No such job, or not yours. Also an unrouted path |
| `CONFLICT` | 409 | No | Idempotency key reused with a different body |
| `PAYLOAD_TOO_LARGE` | 413 | No | Prompt over 100 000 chars, or body over 1 MiB |
| `RATE_LIMITED` | 429 | After `Retry-After` | **Revoye's** ingress limit |
| `JOB_CANCELLED` | 499 | No | Cancelled by you or from the dashboard |
| `INTERNAL_ERROR` | 500 | Yes | Ours. Never carries internal detail |
| `JOB_FAILED` | 502 | Maybe | Attempts exhausted with agent-reported errors. Read `details.attempts` first |
| `NO_DEVICE_ONLINE` | 503 | Yes | No paired device connected |
| `NO_AGENT_AVAILABLE` | 503 | Yes | Device connected, no agent eligible right now |
| `PROVIDER_RATE_LIMITED` | 503 | Yes | **The account's own** hourly cap |
| `JOB_TIMEOUT` | 504 | Yes | Held as long as you asked; **the job continues** |

> `499` is non-standard. Node's `fetch` handles it fine, but some proxies and HTTP client
> abstractions do not — check `res.status === 499` explicitly if you route through one.

### The three that are not failures

These describe the *user's fleet*, not an outage. Retrying them aggressively fights a system
behaving as designed.

- **`NO_DEVICE_ONLINE`** — no paired machine is connected. Only seen with `wait: false` and no
  `callback_url`. Fix: turn a machine on, or stop demanding an immediate answer.
- **`NO_AGENT_AVAILABLE`** — a device is connected but nothing is eligible: every agent is busy,
  disabled, rate-limited, or has already failed this job. Back off. If it persists, the fleet needs
  more agents, not more retries — `GET /v1/status` says which.
- **`PROVIDER_RATE_LIMITED`** — the user's own configured cap. Honour it, or route to another
  provider.

### `RATE_LIMITED` vs `PROVIDER_RATE_LIMITED`

| | Whose limit | What to do |
| --- | --- | --- |
| `RATE_LIMITED` (429) | **Revoye's** ingress, per key, per minute | Respect `Retry-After`. You are sending requests too fast — usually polling |
| `PROVIDER_RATE_LIMITED` (503) | **The user's**, set in their dashboard | Slow down or use another provider. With `wait: true` the job simply waits |

### Practical Node error handling

```js
import { revoye, RevoyeError } from './services/revoye.js';

async function summarise(text, rowId) {
  try {
    const job = await revoye.complete(text, { idempotencyKey: `summary:${rowId}` });

    // A succeeded job with no text means retention pruned it, not that something broke.
    if (job.status === 'succeeded' && job.response === null && job.content_pruned_at) {
      throw new Error(`Job ${job.id} content was pruned at ${job.content_pruned_at}`);
    }
    return job.response;
  } catch (err) {
    if (!(err instanceof RevoyeError)) throw err;

    switch (err.code) {
      case 'UNAUTHORIZED':
      case 'FORBIDDEN':
        // Configuration, not a transient fault. Page someone; do not retry.
        throw new Error(`Revoye credential problem (${err.code}), request ${err.requestId}`);

      case 'INVALID_REQUEST':
      case 'PAYLOAD_TOO_LARGE':
      case 'CONFLICT':
        // Your request is wrong. Retrying it will fail identically.
        throw new Error(`Bad Revoye request: ${err.code} ${JSON.stringify(err.details)}`);

      case 'JOB_TIMEOUT':
        // The job is STILL RUNNING. Do not resubmit — pick it up later.
        if (err.details?.job_id) return { deferred: err.details.job_id };
        throw err;

      case 'JOB_FAILED':
        // Read details.attempts before retrying. A prompt that fails on every agent fails again.
        throw new Error(`Revoye job failed after ${err.details?.attempts} attempts`);

      case 'NO_DEVICE_ONLINE':
      case 'NO_AGENT_AVAILABLE':
      case 'PROVIDER_RATE_LIMITED':
      case 'RATE_LIMITED':
      case 'INTERNAL_ERROR':
        // Already retried inside the client. Requeue for later rather than failing the user.
        throw new Error(`Revoye unavailable (${err.code}); requeue`);

      default:
        throw err;   // unknown code: a new one may ship at any time under /v1
    }
  }
}
```

**Never log** the API key, the `Authorization` header, the prompt, or the response body. Log
`err.code`, `err.status` and `err.requestId`.

---

## 9. Timeouts and retries

### Timeouts

| Call | Client timeout |
| --- | --- |
| `POST /v1/completions` with `wait: true` | **≥ 300 000 ms.** Revoye holds up to `min(deadline_ms, 600000)` — set your client above the `deadline_ms` you send |
| `POST /v1/completions` with `wait: false` | 30 000 ms |
| `POST /v1/chat/completions` | 30 000 ms (it never waits) |
| `GET`/`DELETE` | 30 000 ms |

```js
signal: AbortSignal.timeout(330_000)   // deadline_ms 300_000 + headroom
```

> **The single commonest integration bug** is a 10 s or 30 s default HTTP timeout abandoning a
> healthy job. It looks exactly like Revoye failing, and it is not — the job runs to completion and
> the result waits at `GET /v1/completions/{id}`. Either raise the timeout or stop using
> `wait: true`.

Aborting the request does **not** cancel the job. Use `DELETE /v1/completions/{id}` for that.

### What to retry

| Situation | Retry? |
| --- | --- |
| `NETWORK_ERROR`, `ECONNRESET`, DNS failure, client timeout | Yes — **with the same `Idempotency-Key`** |
| `429 RATE_LIMITED` | Yes, after `Retry-After` seconds |
| `503` (`NO_DEVICE_ONLINE`, `NO_AGENT_AVAILABLE`, `PROVIDER_RATE_LIMITED`) | Yes, with backoff |
| `504 JOB_TIMEOUT` | **Do not resubmit.** The job is still running — poll `details.job_id` |
| `500 INTERNAL_ERROR` | Yes, with backoff |
| `502 JOB_FAILED` | Only after reading `details.attempts`. A prompt every agent refuses will be refused again |
| `400`, `401`, `403`, `404`, `409`, `413`, `499` | **No.** Fix the request or the credential |

### Retrying non-idempotent requests

`POST /v1/completions` and `POST /v1/chat/completions` create work. **Never retry either without an
`Idempotency-Key`** — you will run a second job, spend a second slot against the user's hourly cap,
and get a second answer you did not want.

Generate the key **once per logical operation, before the first attempt**, and reuse it across
every retry:

```js
const idempotencyKey = `report:${reportId}`;   // derived, so a crashed process recomputes it
for (let attempt = 1; attempt <= 4; attempt += 1) {
  try {
    return await revoye.complete(prompt, { idempotencyKey });
  } catch (err) {
    if (!err.retryable || attempt === 4) throw err;
    await sleep(backoff(attempt));
  }
}
```

Regenerating the key inside the loop defeats the entire mechanism. That is the bug to look for when
retries produce duplicate work.

`GET` and `DELETE` are already idempotent and need no key.

### Exponential backoff with jitter

```js
/** 2s, 4s, 8s, 16s… capped at 60s, plus up to 1s of jitter so a fleet does not synchronise. */
function backoff(attempt, retryAfterSeconds) {
  if (retryAfterSeconds) return retryAfterSeconds * 1000;
  return Math.min(2_000 * 2 ** (attempt - 1), 60_000) + Math.random() * 1_000;
}
```

Always prefer the server's `Retry-After` over your own schedule when it is present.

---

## 10. Common integration patterns

### 10.1 Pre-flight capacity check

```js
const status = await revoye.status();

if (status.devices.online === 0) {
  // Nothing can answer now. Queue with a webhook rather than firing a thousand doomed jobs.
  return queueForLater(work);
}
if (status.queue.depth > 100 && status.agents.idle === 0) {
  return queueForLater(work);   // saturated: more agents needed, not more retries
}
```

Poll this on a schedule (once a minute is plenty), never per request.

### 10.2 Synchronous request — interactive scripts and CLIs

```js
const job = await revoye.complete('Explain quorum reads in two paragraphs.', {
  provider: 'claude',
  deadline_ms: 300_000,
});
console.log(job.response);
```

### 10.3 Queue and poll — when you cannot receive inbound requests

```js
const { id } = await revoye.submit(prompt, { idempotencyKey: `doc:${docId}` });
await db.jobs.insert({ docId, revoyeJobId: id, status: 'queued' });

// …later, from a worker on an interval measured in SECONDS, not milliseconds:
const job = await revoye.getJob(id);
if (job.status === 'succeeded') await db.docs.update(docId, { summary: job.response });
else if (['failed', 'cancelled', 'expired'].includes(job.status)) await markFailed(docId, job);
// else still queued/dispatched — check again next tick
```

`revoye.waitForJob(id)` in the client above wraps this loop when you want it inline.

### 10.4 Batch with bounded concurrency

Concurrency is limited by the user's **agents**, not by your process. Firing 500 requests at a
3-agent fleet just fills the queue.

```js
async function mapWithLimit(items, limit, fn) {
  const results = new Array(items.length);
  let cursor = 0;
  const workers = Array.from({ length: limit }, async () => {
    while (cursor < items.length) {
      const i = cursor++;
      results[i] = await fn(items[i], i);
    }
  });
  await Promise.all(workers);
  return results;
}

const { agents } = await revoye.status();
const concurrency = Math.max(1, agents.idle);

const summaries = await mapWithLimit(docs, concurrency, (doc) =>
  revoye.complete(`Summarise:\n\n${doc.body}`, {
    idempotencyKey: `summary:${doc.id}`,
    metadata: { docId: doc.id },
  }),
);
```

### 10.5 Webhooks — the right shape for anything unattended

Submit with `wait: false` and a `callback_url`:

```js
await revoye.submit(prompt, {
  callback_url: 'https://your-app.example.com/hooks/revoye',
  metadata: { releaseId: 'r_2291' },
  idempotencyKey: 'release-notes:r_2291',
});
```

Revoye then `POST`s the finished job to your endpoint.

**Delivery**

```http
POST /hooks/revoye HTTP/1.1
Content-Type: application/json
User-Agent: revoye-webhooks/1
X-Revoye-Signature: sha256=<hex>
X-Revoye-Timestamp: 1755561242
```

Body (note: **no `agent_name`, no `content_pruned_at`, no `queue_position`** — this shape is
slightly narrower than the `GET` response):

```json
{
  "id": "job_01JAY7Q2K8XYZ",
  "status": "succeeded",
  "response": "…",
  "provider": "chatgpt",
  "agent_id": "agt_01JAY7…",
  "conversation_ref": "https://chatgpt.com/c/6f0a…",
  "attempts": 1,
  "queue_ms": 240,
  "run_ms": 18432,
  "created_at": "2026-08-18T09:14:02Z",
  "finished_at": "2026-08-18T09:14:21Z",
  "metadata": { "releaseId": "r_2291" }
}
```

**Endpoint requirements**

| | |
| --- | --- |
| Scheme | **HTTPS only.** An `http://` URL is rejected at submission |
| Address | Must resolve to a public IP. Loopback, RFC 1918, CGNAT, link-local and IPv6 ULA are all refused — so a local tunnel needs a real public hostname |
| Redirects | Not followed |
| Timeout | 10 seconds. A slow handler counts as a failure |
| Response | Any `2xx`. Acknowledge first, work second |
| Retries | At `0 s, 30 s, 5 min, 30 min, 2 h` — **five attempts**, then the delivery is marked failed permanently |

**Verify the signature.** HMAC-SHA256 over the exact string `` `${timestamp}.${rawBody}` `` using
the signing secret Revoye issued you. Verify before you parse.

```js
// src/services/revoye-webhook.js
import { createHmac, timingSafeEqual } from 'node:crypto';

const SECRET = process.env.REVOYE_WEBHOOK_SECRET ?? '';
if (!SECRET) throw new Error('REVOYE_WEBHOOK_SECRET is required');

const MAX_SKEW_SECONDS = 300;

/**
 * @param {Buffer} rawBody    The exact bytes received. NOT a re-serialised object.
 * @param {string} signature  X-Revoye-Signature header, "sha256=<hex>"
 * @param {string} timestamp  X-Revoye-Timestamp header, unix seconds
 */
export function verifyRevoyeWebhook(rawBody, signature, timestamp) {
  if (!signature || !timestamp) return false;

  const skew = Math.abs(Date.now() / 1000 - Number(timestamp));
  if (!Number.isFinite(skew) || skew > MAX_SKEW_SECONDS) return false;   // reject replays

  const expected =
    'sha256=' +
    createHmac('sha256', SECRET).update(`${timestamp}.`).update(rawBody).digest('hex');

  const a = Buffer.from(expected);
  const b = Buffer.from(signature);
  return a.length === b.length && timingSafeEqual(a, b);   // constant time
}
```

**Express** — capture the raw body, or the signature can never match:

```js
import express from 'express';
import { verifyRevoyeWebhook } from './services/revoye-webhook.js';

const app = express();

app.post(
  '/hooks/revoye',
  express.raw({ type: 'application/json', limit: '2mb' }),
  async (req, res) => {
    const ok = verifyRevoyeWebhook(
      req.body,                              // Buffer, because of express.raw
      req.get('x-revoye-signature'),
      req.get('x-revoye-timestamp'),
    );
    if (!ok) return res.status(401).end();

    const job = JSON.parse(req.body.toString('utf8'));

    // Acknowledge FIRST — the sender's timeout is 10 s and a slow handler earns a retry.
    res.status(200).end();

    // Delivery is at-least-once: make a repeat of the same id a no-op.
    if (await alreadyProcessed(job.id)) return;
    await processJob(job);
    await markProcessed(job.id);
  },
);
```

Mount `express.json()` **after** this route, or scope it to other paths — a global JSON parser
consumes the raw bytes and the signature will never verify.

**Fastify**:

```js
fastify.addContentTypeParser(
  'application/json',
  { parseAs: 'buffer' },
  (req, body, done) => done(null, body),   // keep the raw bytes
);

fastify.post('/hooks/revoye', async (request, reply) => {
  const ok = verifyRevoyeWebhook(
    request.body,
    request.headers['x-revoye-signature'],
    request.headers['x-revoye-timestamp'],
  );
  if (!ok) return reply.code(401).send();

  const job = JSON.parse(request.body.toString('utf8'));
  void reply.code(200).send();
  await enqueueForProcessing(job);   // work happens off the request path
});
```

**Next.js App Router** (`app/api/hooks/revoye/route.js`):

```js
export const runtime = 'nodejs';   // node:crypto is required

export async function POST(request) {
  const raw = Buffer.from(await request.arrayBuffer());   // raw bytes, before any parsing
  const ok = verifyRevoyeWebhook(
    raw,
    request.headers.get('x-revoye-signature'),
    request.headers.get('x-revoye-timestamp'),
  );
  if (!ok) return new Response(null, { status: 401 });

  const job = JSON.parse(raw.toString('utf8'));
  await enqueueForProcessing(job);
  return new Response(null, { status: 200 });
}
```

**Three mistakes, in order of frequency:**

1. Verifying against the **parsed** body. Re-serialising JSON changes whitespace and key order and
   the signature will never match. Capture the raw bytes.
2. Comparing with `===`. Use `timingSafeEqual`.
3. Skipping the timestamp check, which leaves a captured delivery replayable forever.

### 10.6 Continuing a conversation

`conversation_ref` comes back on a completed job. Pass it with `mode: "continue"` to keep the same
thread in the provider's own UI:

```js
const first = await revoye.complete('Explain the CAP theorem.');

const second = await revoye.complete('Now give a concrete example of a CP system.', {
  mode: 'continue',
  conversation_ref: first.conversation_ref,
  idempotencyKey: `follow-up:${first.id}`,
});
```

`mode: "continue"` without `conversation_ref` is `400 INVALID_REQUEST` with
`details.field: "conversation_ref"`.

Revoye holds no conversation state of its own — the handle points into the provider's UI. It can be
`null` on a job that never dispatched.

### 10.7 Correlating results with your own records

`metadata` is echoed back verbatim on the completion **and** on the webhook, and it is excluded from
the idempotency fingerprint. Put your tenant id, row id or trace id there and route a result that
arrives an hour later with no database lookup.

```js
await revoye.submit(prompt, {
  callback_url: HOOK_URL,
  metadata: { tenantId, rowId, traceId },   // ≤ 4 KiB serialised
});
```

### 10.8 Model picker

```js
const { data } = await revoye.listModels();
const options = data.map((m) => m.id);   // ["revoye/auto", "chatgpt", "deepseek"]
```

Empty `data` with a `_revoye.note` means no providers are configured or all are disabled — surface
that message rather than an empty dropdown.

### 10.9 Cancellation on user abort

```js
const { id } = await revoye.submit(prompt);
abortController.signal.addEventListener('abort', () => {
  void revoye.cancelJob(id);   // aborting your HTTP request alone does NOT cancel the job
});
```

### 10.10 Pagination and filtering

**The public API has no list endpoints.** There is no `GET /v1/completions` collection, and nothing
under `/v1` is paginated or filterable. Cursor pagination (`?limit=`, `?cursor=`, `next_cursor`)
belongs to the dashboard API, which an API key cannot reach.

Keep your own index: store each `job.id` alongside your record at submission time, and query your
own database. Do not attempt to enumerate jobs through Revoye.

---

## 11. AI agent integration instructions

If you are an AI coding agent asked to integrate Revoye into a Node.js application:

1. **Read this file first**, before writing any Revoye code. Do not go looking for the API shape
   elsewhere.
2. **Pick the endpoint from §4.** There are exactly six. If the task needs something not listed
   there, it is not in the public API — say so rather than inventing a path.
3. **Reuse the client in §6.** Create `src/services/revoye.js` (or the project's equivalent
   location) once and call methods from application code. Do not scatter `fetch` calls with
   hand-written headers.
4. **Never invent endpoints, parameters, response fields, error codes or auth mechanisms.** If a
   field is not in §4, it does not exist. If asked for behaviour Revoye does not have (streaming,
   token counts, listing jobs, per-request model parameters like temperature), state the limit
   plainly.
5. **Follow the documented schemas exactly.** Public API fields are `snake_case`. Respect the value
   ranges in §5 — an out-of-range `timeout_ms` is a `400`, not a clamp.
6. **Default to `POST /v1/completions`.** Use `/v1/chat/completions` only when an existing
   OpenAI-shaped client must keep compiling, and then add the polling step from §4.4 — it never
   returns an answer directly.
7. **Keep the key server-side.** Never place it in client-side code, a `NEXT_PUBLIC_`/`VITE_`
   variable, a mobile bundle, or a committed file. If the request implies a browser calling Revoye
   directly, add a server route instead and explain why.
8. **Implement real error handling.** Branch on `error.code`, never on `error.message`. Distinguish
   retryable from terminal per §8. Log `request_id`; never log keys, prompts or responses.
9. **Set timeouts explicitly.** `wait: true` needs ≥ 300 s. A default timeout is the most common
   failure in Revoye integrations.
10. **Always send `Idempotency-Key` on `POST`,** derived from the caller's own work id where one
    exists, generated once per logical operation otherwise. Never regenerate it inside a retry loop.
11. **Follow the host application's existing architecture** — its config loading, logging, error
    types, HTTP layer and queueing. Match the surrounding code's style.
12. **Add no dependencies.** Node 18+ global `fetch`, `AbortSignal.timeout` and `node:crypto` cover
    everything here. Do not pull in axios, node-fetch, got or an SDK.
13. **Design for latency.** 20–90 seconds per job. If the code path is behind a user-facing spinner
    or an HTTP request with a short budget, queue the work and return a job id instead.
14. **Handle `NO_DEVICE_ONLINE` / `NO_AGENT_AVAILABLE` / `PROVIDER_RATE_LIMITED` as fleet states**,
    not outages — back off, or queue with a webhook.

---

## 12. Security best practices

- **Never expose the API key in frontend JavaScript.** No `NEXT_PUBLIC_REVOYE_API_KEY`, no
  `VITE_*`, no mobile bundle. `/v1/*` sends permissive CORS headers for tooling; that is not an
  invitation. Proxy through your own authenticated server route.
- **Never commit secrets.** `.env` in `.gitignore`; commit `.env.example` with empty values. The
  `revoye_sk_live_` prefix is registered with secret scanners — a committed key will be flagged, and
  it will also already be public.
- **Use environment variables or a secret manager**, and load them at boot so a missing key fails
  fast instead of failing inside a worker.
- **One key per deployment**, scoped as narrowly as the workload allows: submit-only workers get
  `completions:write`; monitoring gets `status:read`.
- **Rotate deliberately:** create → deploy → confirm traffic on the new key in the dashboard →
  revoke the old. Revocation is immediate with no grace period, so confirmation comes before
  revocation. Rotate immediately on any suspicion — a key in a log, a repository, a screenshot, a
  support thread.
- **Validate external input before it becomes a prompt.** Cap length (100 000 chars is the hard
  limit), and treat user text as untrusted — anything you put in `prompt` reaches a real provider
  account belonging to the user.
- **Treat `response` as untrusted output.** It is model-generated text. Never `eval` it, never
  interpolate it into SQL or shell commands, and escape it before rendering as HTML.
- **Never log the `Authorization` header, the key, the prompt, or the response.** Many frameworks
  log headers by default — redact explicitly. Log `error.code`, HTTP status and `request_id`.
- **Verify every webhook** (§10.5): HTTPS endpoint, HMAC check on the raw bytes, constant-time
  comparison, timestamp skew check, idempotent handler.
- **HTTPS everywhere.** The API is HTTPS-only and `callback_url` must be HTTPS.
- **Least privilege:** an API key already cannot touch account settings, keys, devices or the audit
  log. Keep it that way by not proxying dashboard operations through your integration.
- **Handle errors safely.** Return your own generic message to end users; keep `request_id` and
  `error.code` in your logs. Never surface a raw Revoye error body to a browser.

---

## 13. Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| `401 UNAUTHORIZED` | Missing/malformed/expired/revoked key | Check the header is exactly `Authorization: Bearer revoye_sk_…`. Confirm the key is active in the dashboard |
| `403 FORBIDDEN`, `details.required_scope` | Key lacks the scope | Create a key with `completions:write` / `completions:read` / `status:read` |
| `403 FORBIDDEN`, `details.limit: 1000` | Account is at its queued-job ceiling | Stop submitting; drain the queue |
| `404 NOT_FOUND` | Wrong job id, a job belonging to another account, or a mistyped path | Check the id and the path. `404` is deliberately ambiguous between "missing" and "not yours" |
| `400 INVALID_REQUEST` | Validation. `details.field` names it | Common: `stream: true`; unknown `model`; `mode: "continue"` without `conversation_ref`; out-of-range `timeout_ms`/`deadline_ms`/`priority` |
| `409 CONFLICT` | `Idempotency-Key` reused with a different body | Almost always a bug in key derivation — two operations computing the same key. Fix the derivation, do not just use a fresh key |
| `413 PAYLOAD_TOO_LARGE` | Prompt > 100 000 chars, flattened messages > 100 000 chars, or body > 1 MiB | Chunk the input |
| `429 RATE_LIMITED` | Ingress limit (600 writes/min, 1 200 reads/min per key) | Honour `Retry-After`. Usually caused by polling too fast — look at your polling before your prompts |
| `499 JOB_CANCELLED` | Cancelled by you or from the dashboard | Not an error to retry |
| `500 INTERNAL_ERROR` | Revoye-side | Retry with backoff. Quote `request_id` in support |
| `502 JOB_FAILED` | Every attempt failed with agent-reported errors | Read `details.attempts`. A prompt a provider refuses will be refused again |
| `503 NO_DEVICE_ONLINE` | No paired machine connected (only with `wait: false` and no callback) | Turn the machine on, or submit with `wait: true` / a `callback_url` so it queues |
| `503 NO_AGENT_AVAILABLE` | Connected, but nothing eligible | Back off. Check `GET /v1/status` — if `agents_idle` is persistently 0, the fleet needs more agents |
| `503 PROVIDER_RATE_LIMITED` | The user's own hourly cap | Wait, or use a different provider |
| `504 JOB_TIMEOUT` | The hold expired | **The job is still running.** Poll `details.job_id`; do not resubmit |
| Network error / `TimeoutError` after ~30 s on `wait: true` | Your HTTP client timeout is too low | Raise it above `deadline_ms` (≥ 300 s). The job did not fail — fetch it by id |
| `ECONNRESET` / socket hang-up mid-`wait` | A proxy, load balancer or VPN closed a long-lived connection | Prefer `wait: false` + webhook; retry with the same `Idempotency-Key` |
| Duplicate jobs after retries | The idempotency key was regenerated per attempt | Generate it once per logical operation, outside the retry loop |
| `status: "succeeded"` but `response: null` | `content_pruned_at` is set — retention removed the text | Fetch results sooner, or raise the account's retention setting |
| OpenAI SDK returns an empty message | `/v1/chat/completions` always returns `202` with `content: null` | Poll `GET /v1/completions/{_revoye.job_id}`, or switch to `/v1/completions` |
| Webhook signature never matches | Verified against a parsed/re-serialised body | Capture raw bytes before any body parser (§10.5) |
| Webhook never arrives | `callback_url` is not HTTPS, resolves to a private IP, redirects, or the endpoint took > 10 s | Public HTTPS host, no redirects, `2xx` immediately, work asynchronously |
| `GET /v1/completions/{id}?wait=true` returns instantly | The query parameter is not implemented | Poll, or use a webhook |
| Jobs queue forever, `devices.online: 0` | The user's machine is asleep or Revoye Desk is not running | Nothing to fix in code — this is a fleet state |
| Everything is slow | Throughput is agents, not bandwidth: roughly `agents × (3600 / seconds per prompt)` per hour | More agents, or fewer prompts. Not more retries |

**Always capture `X-Request-Id`** on a failure. It is the difference between a support conversation
and a guess.

---

## 14. API coverage verification

This file was written against the Revoye API implementation and its published documentation, not
from memory. The public surface is exactly:

| Method | Path |
| --- | --- |
| `POST` | `/v1/completions` |
| `GET` | `/v1/completions/{id}` |
| `DELETE` | `/v1/completions/{id}` |
| `POST` | `/v1/chat/completions` |
| `GET` | `/v1/models` |
| `GET` | `/v1/status` |

Plus one outbound flow: the signed `callback_url` webhook.

**Deliberately excluded** — not part of the public API and unreachable with an API key:

- The dashboard API (`/api/*`): auth, sessions, profile, API-key management, devices, providers,
  agents, job history, usage, events, admin. Session cookie + CSRF only.
- The device gateway (`/v1/pair`, `/v1/challenge`, `/v1/agent`): pairing codes and Ed25519, used by
  Revoye Desk.
- Health and metrics endpoints.

**Where this file corrects or sharpens the published docs** (verified against the implementation):

- `GET /v1/completions/{id}?wait=true` is documented publicly but **not implemented** — the query
  parameter is ignored.
- `POST /v1/chat/completions` **never waits**; it always returns `202` with `content: null`. A stock
  OpenAI SDK will not receive an answer without a follow-up poll.
- `agent_name` is always `null` on the public API, despite example payloads showing a name.
- `queue_position` is the account's **queue depth**, i.e. an upper bound, not an exact index.
- `content_pruned_at` is part of every `/v1/completions` response and is absent from the published
  examples.
- The webhook body is narrower than the `GET` response: no `agent_name`, `content_pruned_at` or
  `queue_position`.
- `POST /v1/completions` can also return `403 FORBIDDEN` when the account is at its 1 000-queued-job
  ceiling.

Base URLs and limits are deployment configuration. `https://api.revoye.com` is the production
host for this deployment; a self-hosted instance differs, which is why §7 keeps it in an environment
variable. `MAX_PROMPT_CHARS`, `MAX_BODY_BYTES` and `MAX_QUEUED_JOBS_PER_USER` are operator-tunable —
the values in §5 are the production defaults.
