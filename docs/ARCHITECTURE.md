# Architecture

SVNpush is a Tauri 2 desktop app. The release engine is a plain Rust
library, the desktop shell is a thin Tauri layer over it, and the window is
a React app that renders the state the engine emits. The specification is
[PLAN.md §4](PLAN.md). Choices it left open are in [DECISIONS.md](DECISIONS.md).

```
┌──────────── apps/desktop/src (React, TypeScript) ────────────┐
│ screens · components · stores (zustand) · strings.ts · CSS   │
│ ipc/commands.ts ──invoke──┐        ┌──listen── ipc/events.ts │
└───────────────────────────┼────────┼─────────────────────────┘
┌──────────── apps/desktop/src-tauri (shell) ──────────────────┐
│ commands.rs → service/{projects,runs,providers,vault,settings}│
│ state.rs (AppState) · events.rs (run-state, run-log)         │
│ logging.rs (redacted, rotated files) · capabilities/         │
└───────────────────────────┬──────────────────────────────────┘
┌──────────── crates/core (svnpush-core) ──────────────────────┐
│ run ─ detect · readme · version · edit · package · verify    │
│     ─ svn · tools · vault · secret · project · settings      │
│     ─ ai (provider, adapters, registry, router, client, …)   │
└──────────────────────────────────────────────────────────────┘
        │ svn / git processes      │ HTTPS           │ OS keychain
```

## Repository layout

| Path | What it is |
|---|---|
| `crates/core` | `svnpush-core`: every rule, check and side effect of a release. No Tauri dependency. |
| `apps/desktop/src-tauri` | `svnpush-desktop`: Tauri commands, app state, events, logging, updater. |
| `apps/desktop/src` | The React UI. |
| `fixtures/` | Plugins, readmes and recorded AI responses used by tests. |
| `docs/` | Plan, decisions, protocol, this file, release process, Revoye API. |
| `.github/workflows` | `check` (fmt, clippy, deny, lint, typecheck), `test`, `svn-integration`, `build`. |

## Core (`crates/core/src`)

| Module | Responsibility |
|---|---|
| `detect` | Plugin header, slug, main file candidates, git facts. |
| `readme` | Parses `readme.txt` and edits it byte-for-byte (headers, changelog, upgrade notice). |
| `version` | Version parsing and ordering, reading and writing every version source. |
| `edit` | `EditSet`: composes edits to files and writes them once. |
| `package` | Exclusion rules (`ignore` crate), staging, deterministic zip, SHA-256. |
| `verify` | V01–V16 and W01–W10 as pure functions over a `VerifyInput`. |
| `tools` | Finding `svn` and `git`, and running processes with cancellation and log streaming. |
| `svn` | Sparse working copy, mirror sync, status XML, commit, tag, verification. Errors are classified by `E` codes. |
| `vault` | `CredentialStore` over the OS keychain (`keyring`), SVN accounts, AI key names. |
| `secret` | `Secret` (never printed, cloned or serialised) and `redact_secrets`. |
| `project`, `settings` | App data paths, `projects.json`, `.svnpush.json` overlay, `settings.json`, WordPress version lookup. |
| `ai` | The AI layer (below). |
| `run` | The release state machine (below). |
| `report`, `clock`, `error`, `text` | Log lines, time, the `Coded` error trait, text helpers. |

Every error type implements `Coded`: a stable `code`, a sentence, and a fix.
The UI shows those three fields and nothing else.

### The run engine (`run/`)

- `engine.rs`: `Run::prepare` takes the project lock and starts the journal.
  `Run::execute` drives the steps and handles stop, rollback and the end of
  a dry run.
- `steps.rs` (Detect, Draft, Write, Verify), `svn_steps.rs` (Build, Preview),
  `publish.rs` (Publish, Resume, Discard), `assets.rs` (assets-only
  release).
- `assist.rs` and `explain.rs`: the AI draft and the failed-check
  explanation. An AI task runs as an owned future while the run keeps
  reading decisions, so you can stop or redirect it. Progress arrives over
  an in-process channel.
- `material.rs`: the diff material and filters sent to the AI. `changes.rs`
  and `draft.rs`: the change set and the manual pre-fill.
- `journal.rs`, `snapshot.rs`, `lock.rs`, `hook.rs`: persistence, rollback,
  the per-plugin lock and the pre-build command.
- `model.rs` and `ai_view.rs`: `RunState` and everything in it, exported to
  TypeScript.

A run talks to the outside through `RunInputs` (paths, project, tool
discovery, keychain, accounts, AI client) and `RunObserver` (state and log
lines). You answer it with `Decision` values over a channel: `Approve`,
`Generate`, `AcceptPrivacy`, `Manual`, `ApplyFixes`, `Stop`, `Publish`.
Cancellation is a `CancellationToken`.

### The AI layer (`ai/`)

- `provider.rs`: the provider contract. It defines normalised requests and
  results, error codes, and the traits an adapter implements (build the HTTP
  request, parse the response, and optionally poll, list models, report
  fleet status).
- `adapters/`: Revoye (asynchronous jobs), Gemini, Claude, OpenAI, OpenRouter,
  and the OpenAI-compatible family (DeepSeek, Qwen, Perplexity, custom,
  local).
- `registry.rs` and `order.rs`: the adapter list, with Revoye first.
- `client.rs`: sends adapter requests with timeouts, runs the poll loop, and
  cancels jobs server-side. It logs provider, status, request id and code
  only.
- `records.rs`: `providers.json` (no keys), monthly counts, "needs
  attention".
- `router.rs`: resolves the provider and walks the opt-in fallback order.
  `task.rs`: validates answers against JSON schemas with one retry.
- `prompts.rs` and `schemas.rs`: the three tasks, `draft_release`,
  `summarise_file` and `explain_failures`.

Adapters are pure request builders and response parsers. They are tested
against recorded fixtures. The client and router are tested against a
mock server.

## Shell (`apps/desktop/src-tauri/src`)

- `lib.rs` builds the app: logging, the keychain store, the AI client,
  plugins (dialog, opener, updater) and the command list.
- `commands.rs` holds one `#[tauri::command]` per action. Each forwards to
  `service/`, which holds the logic as plain functions over `AppState`, so it
  is unit-tested without a window.
- `service/runs.rs` starts a run on the async runtime, stores the latest
  `RunState` per project, and forwards decisions. `events.rs` emits
  `run-state` (the whole state) and `run-log` (one line) to the window.
- `logging.rs` writes daily-rotated files (seven kept) through a writer that
  removes anything shaped like a key or token before it reaches disk.

## UI (`apps/desktop/src`)

- `screens/` holds the six screens: Projects, Release, Providers, Vault,
  Settings and Help. Help opens on first launch with the setup checklist.
  `components/` has one component per file.
- `store/` holds zustand stores for projects, runs and providers. The run
  store keeps the last `RunState` and up to 2,000 log lines per project, and
  contains no workflow logic.
- `ipc/` has the typed `invoke` wrapper (`CommandError` carries `{ code,
  message, fix }`), the command table and event listeners. Its types are
  generated from Rust with `ts-rs` into `ipc/bindings` (not committed).
- `strings.ts` holds every user-visible string. `styles/` is plain CSS with
  design tokens and light and dark values.

## Security model

- **Secrets** live only in the OS keychain (service `svnpush`, accounts
  `svn:<host>:<user>` and `ai:<provider id>`). They are read right before use
  and dropped right after. `Secret` cannot be printed or serialised. Nothing
  secret is written to a log, a config file, a journal or the UI.
- **svn** receives the password on stdin (`--password-from-stdin`,
  `--no-auth-cache`, `--non-interactive`), never as an argument.
- **AI output** is untrusted text: rendered as text, validated against a
  schema, never executed, and never written without your approval. Only exact
  readme replacements can be applied, and each is shown as a diff first.
- **Prompts** never include files matching the exclude patterns or files
  that look like secrets.
- **Webview**: a strict CSP (`'self'` only). Least-privilege capabilities:
  the app's own commands, event listen and unlisten, the folder dialog,
  opening `https://wordpress.org/plugins/*`, and revealing a built zip.
- **Updates** are signed. The updater accepts only artifacts signed with the
  private key matching the compiled-in public key.

## Data on disk

`<app-data>/svnpush/`: `projects.json`, `providers.json`, `accounts.json`
(usernames only), `settings.json`, `wc/<slug>/` (sparse working copies),
`builds/<slug>/<version>/`, `runs/<slug>/<run id>.json` and snapshots, and
`logs/`. A project may add `.svnpush.json` for team settings; it is never
packaged.

## Testing

- Unit tests sit beside the code in both crates.
- `crates/core/tests` holds fixture-driven detection, readme, package and
  verify suites, AI adapter fixtures, the mock-server client tests, and the
  SVN integration suite against a local `file://` repository (feature
  `svn-integration`).
- Frontend tests use vitest and Testing Library against a scripted Tauri
  mock (`src/test/tauriMock.ts`).
