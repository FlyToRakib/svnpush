# SVNpush — Development Prompt

Copy everything below the line into a fresh Claude Code session opened at
the root of the `svnpush` repository. It drives the whole build.

---

You are building **SVNpush**, a desktop app that takes any WordPress plugin
from a local folder to a published WordPress.org release in one click. You
are working alone, autonomously, from the plan in this repository, and you
will not stop until the definition of done in that plan is met.

## Sources of truth

1. `docs/PLAN.md` is the specification. Read it completely before writing
   any code and re-read the relevant section before starting each
   milestone. Where the plan is explicit, follow it exactly. Where it is
   silent, decide the way a careful senior engineer would, write the
   decision as one dated line in `docs/DECISIONS.md`, and continue.
2. `docs/REVOYE-API.md` is the only reference for the Revoye adapter. It
   describes six endpoints. Do not invent an endpoint, parameter, field,
   error code or behaviour that is not in that file. If something is not
   there, it does not exist.
3. Reference code you may read and port, never copy blindly:
   - `F:\agi\syncdock\syncdock_client\shared\js\services\ai\` — the AI
     provider pattern (`provider.js`, `registry.js`, `providerOrder.js`,
     `router.js`, `adapters/*.js`). Port the shape, the names, the error
     codes and the per-provider quirks to Rust as section 9 of the plan
     describes. Leave out streaming, image generation and USD pricing.
   - `F:\agi\syncdock\syncdock_client\pages\providers\` — the Providers
     screen. Rebuild it in React with the same fields, visibility rules and
     behaviours, with Revoye first and marked "(Recommended)".
   - `F:\agi\AuthDock\` — a real, shipping WordPress plugin. Use it to learn
     WordPress conventions: `readme.txt`, the plugin header in
     `authdock.php`, `.distignore`, `bin/check-versions.php`,
     `bin/build.ps1`, `docs/SVN-DEPLOY.md`. Copy its `readme.txt` and the
     header of `authdock.php` into `fixtures/plugins/real-world/` as a test
     fixture. Do not copy the rest of the plugin and do not modify that
     repository.
4. For anything else, use the official documentation and nothing weaker:
   Tauri 2, the Subversion book and `svn help`, the WordPress.org Plugin
   Handbook (Subversion, readme, Plugin Check), and each AI vendor's own
   API reference for Gemini, Claude, OpenAI and OpenRouter. Verify a model
   id or an endpoint before you use it. Never guess a wire format.

## Operating rules

- **Never stop before the definition of done.** Do not pause to ask a
  question, request confirmation, or offer options. Decide, record in
  `docs/DECISIONS.md`, continue. The owner will review the finished app
  manually.
- **Deliver milestones in order**, M0 to M7 as the plan defines them. A
  milestone is complete only when its "Done when" condition is met, the
  full CI check set passes locally (`cargo fmt --check`, `cargo clippy`,
  `cargo test`, `npm run lint`, `npm run typecheck`, `npm test`), and a
  Conventional Commit has been made for it. Do not start the next one
  before that.
- **No placeholders.** No `TODO`, no `unimplemented!`, no stubbed function
  that returns a fake value, no "coming soon" in the UI, no commented-out
  code. If something cannot be finished, it is a bug to fix now, not a note
  for later.
- **No scope creep.** Build exactly the five screens, the seven steps, the
  sixteen blocking checks, the ten warnings, and the ten adapters in the
  plan. Do not add a feature, a screen, a setting or a dependency the plan
  does not call for. If you believe one is needed, add it to
  `docs/DECISIONS.md` with the reason and implement the smallest version.
- **No bloat.** Every dependency must earn its place; list each new crate
  or npm package in `docs/DECISIONS.md` with one line of justification.
  Prefer the standard library. No UI component library, no CSS framework,
  no state-management framework beyond the one small store the plan names.
- **Clean code is the deliverable.** Follow section 14 of the plan
  exactly: `clippy -D warnings` with the pedantic group, no `unwrap` or
  `expect` outside tests, `thiserror` errors with stable codes, `tracing`
  with a redaction layer, modules under 500 lines, one component per file,
  strict TypeScript, DTOs generated with `ts-rs`, plain CSS tokens, every
  string in `strings.ts`. Read your own diff before each commit and remove
  anything that is not needed.
- **Security is not optional.** Secrets go only to the OS keychain. Nothing
  secret reaches a log, a file, a journal, the UI, or a test fixture. `svn`
  receives the password over stdin. AI output is untrusted text. The
  webview has a strict CSP. Tauri capabilities are least-privilege.
- **Test what you build, as you build it.** Every parser, every check,
  every adapter and the router get unit tests with fixtures. The SVN engine
  gets the `file://` integration suite. The AI client gets the mock-server
  lifecycle tests. Failing tests are never skipped or weakened to pass.
- **Verify in the real app.** After M4, run the app, add a fixture plugin,
  and complete a dry run from the UI. After M6, complete a real AI draft.
  Fix what you see. A feature that passes tests but fails in the window is
  not done.
- **Commit discipline.** Conventional Commits, one logical change per
  commit, message body explains why. Keep `CHANGELOG.md` current. Never
  commit a secret, a build artefact, or a generated file other than
  lockfiles.

## Environment

- Windows 11, PowerShell. Rust via `rustup` (stable), Node 20 LTS, the
  Tauri 2 prerequisites (Microsoft Visual Studio C++ Build Tools, WebView2
  runtime). Install anything missing yourself with `winget` or `choco`
  and note it in `docs/DECISIONS.md`.
- Subversion must be on PATH for the integration suite. If it is not,
  install it (`choco install svn` or TortoiseSVN with command-line tools)
  before M3.
- Work in the repository root. Use the scratchpad directory for anything
  temporary. Never write outside the repository except to the scratchpad
  and the app's own data directory during manual runs.

## Order of work

**M0 Scaffold.** Cargo workspace with `crates/core` and
`apps/desktop/src-tauri`; Vite + React + TypeScript in `apps/desktop`;
`tauri.conf.json` with CSP, window minimum 960×640, updater placeholders;
design tokens with light and dark and the three-way theme toggle;
`strings.ts`; ESLint, Prettier, rustfmt, clippy config, `cargo deny`;
GitHub Actions `check`, `test`, `build`; `README.md`, `CONTRIBUTING.md`,
`CHANGELOG.md`, `docs/DECISIONS.md`. The app opens a blank shell with the
five-screen navigation and the theme toggle working.

**M1 Detect and versions.** `detect`, `version`, `readme` modules with the
fixtures the plan lists. Round-trip tests. The `Header` and `Readme` types
generated to TypeScript.

**M2 Package and verify.** `package` with `.distignore` semantics via the
`ignore` crate, the built-in defaults, the hard-excluded list, staging with
BLAKE3 hashes, deterministic zip, checksum. `verify` with V01–V16 and
W01–W10, each with a passing and a failing test.

**M3 SVN.** `tools` discovery with the version gate and the known install
paths; `svn` with sparse checkout, update, sync, status, mime-type props,
commit over stdin, server-side tag copy, `ls` and `cat` verification,
cleanup and reset. The `file://` integration suite with `svnadmin`.

**M4 Desktop release flow.** `run` state machine, journal, snapshot and
rollback, lock file, cancel and resume. Tauri commands and events. Projects
and Release screens, the seven step cards, the log drawer, the manual draft
form, dry run, the publish confirmation. Verify with a real dry run in the
window.

**M5 Vault, Providers, Settings.** `vault` over `keyring`. Vault screen
with SVN accounts and Test. `providers.json` records. The Providers screen
as a faithful port of the SyncDock page, driven entirely by each adapter's
`AdapterMeta`: with Revoye first and "(Recommended)", the model dropdown or
free text, the ↻ load, the fleet line, the write-only key, the hidden base
URL for fixed endpoints, the keyless case, the default toggle, Test, Remove,
Clear attention, and the opt-in fallback list. Settings with Doctor, theme,
privacy toggle, Copy diagnostics, version and update check.

**M6 AI engine.** `ai/provider.rs`, `registry.rs`, `order.rs`, `router.rs`,
`client.rs`, `prompts.rs`, `schemas.rs`, and the adapters: `revoye.rs`
first, from `docs/REVOYE-API.md` alone; then `openai.rs` with the
`OpenAiCompatible` factory and its six instances; then `gemini.rs`,
`claude.rs`, `openrouter.rs`. Recorded fixtures per provider under
`fixtures/ai/`. The mock-server lifecycle tests. Wire the Draft step to the
router, the provider "Change" link, the Revoye queue position and cancel,
the schema-validate-and-retry, the explain-failures pass, the one-time
privacy notice, and AI-off mode. Verify with a real draft in the window.

**M7 Polish and 1.0.** Assets-only release, keyboard and accessibility
pass, reduced motion, diagnostics export, `docs/PROTOCOL.md`,
`docs/ARCHITECTURE.md`, `docs/RELEASE.md`, `tauri-action` build workflow
for the three platforms, updater manifest, final README with screenshots
placeholders replaced by real screenshots taken from the running app.

## When you are done

Produce `docs/BUILD-REPORT.md` containing: what was built per milestone,
the exact commands to run the app and the tests, the test counts and
results, every entry from `docs/DECISIONS.md` summarised, known
limitations, and what a manual reviewer should try first. Then stop.
