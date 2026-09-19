# Build Report

SVNpush was built from [PLAN.md](PLAN.md) following
[DEVELOPMENT-PROMPT.md](DEVELOPMENT-PROMPT.md), milestones M0 to M7 in
order. This report covers:

- what each milestone delivered
- how to run the app and the tests
- the test results
- a summary of every decision in [DECISIONS.md](DECISIONS.md)
- known limitations
- what a reviewer should try first

Toolchain used: Rust 1.98.1 (the workspace requires 1.89 or newer), Node
22.22, Subversion 1.14.5 and git 2.54, on Windows 11.

## What was built, per milestone

| Milestone | Commit | Delivered |
|---|---|---|
| M0 Scaffold | `3d761e8` | Cargo workspace (`svnpush-core`, `svnpush-desktop`), npm workspace, Tauri 2 shell with strict CSP and least-privilege capabilities, React 19 + TypeScript app shell with the five-screen navigation and the System/Light/Dark toggle, design tokens, ESLint/Prettier/Clippy (pedantic)/rustfmt/cargo-deny, CI workflows (check, test, build). |
| M1 Detect and versions | `8f28697` | Plugin header and slug detection, main-file candidates, the byte-preserving `readme.txt` parser and writer (headers, changelog, upgrade notice), version parsing/ordering and every version source (header, Stable tag, custom regex locations), `EditSet`, the `Coded` error trait, fixtures including AuthDock's readme and header. |
| M2 Package and verify | `3ce0cb8` | `.distignore`/default/hard exclusion rules, staging, deterministic zip with SHA-256, build pruning, all sixteen blocking checks V01–V16 and ten warnings W01–W10 with a failing and a passing test each. |
| M3 SVN | `8daf596` | Tool discovery, a cancellable process runner with streamed output, the sparse working copy, mirror sync in batches, status XML parsing, commit, server-side tag (refusing an existing tag), tag verification with retries, `E`-code error classification, the password over stdin, `Secret`, and the SVN integration suite against `file://`. |
| M4 Release flow without AI | `dafb436` | The run engine (seven steps, journal, snapshot and rollback, OS file lock, cancel, dry run, Resume after a trunk commit, Discard), keychain vault core, the Projects and Release screens, run store, log drawer, past releases, project settings, the manual draft form. Verified in the real window: a dry run. |
| M5 Vault, Providers, Settings | `2ca37f3` | Vault screen (SVN accounts in the OS keychain, Test), Providers screen driven by adapter metadata (all ten adapters, model lists, Revoye fleet, Test, default, needs-attention, opt-in fallback order), Settings (theme, tool paths with Doctor, WordPress version lookup toggle for W02, Copy diagnostics, update check), redacted rotating logs, the signed updater. Verified in the real window: a publish to a local `file://` repository (trunk r2, tag r3, verified) with the password in Windows Credential Manager. |
| M6 AI engine | `495f328` | Prompts and JSON schemas for `draft_release`, `summarise_file` and `explain_failures`; validation with one retry; the router (override → pinned → default → only record) with the opt-in fallback order; the AI draft in Step 2 (privacy notice, Change link for the run, Revoye queue position and fleet, stop that cancels the Revoye job, version override, summaries for large diffs, AI-off mode); explaining failed checks with applicable readme-only fixes; mock-server lifecycle tests. Verified in the real window against a local OpenAI-compatible stub (see limitations). |
| M7 Polish and 1.0 | `b584241`, `d405278` and the two commits after it | Assets-only release ("Update assets"), accessibility pass (inline privacy notice, 3:1 input borders, AA text contrast in both themes, phase announcements, keyboard walk-through, aligned field grids), docs ([PROTOCOL.md](PROTOCOL.md), [ARCHITECTURE.md](ARCHITECTURE.md), [RELEASE.md](RELEASE.md)), the `tauri-action@v1` build workflow with updater signing and `latest.json`, README with screenshots from the running app, and a fix for plugins inside a larger git repository. |

## Commands

Prerequisites: Rust stable, Node 22.22+, the
[Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/),
Subversion 1.10+ (with `svnadmin` for the integration suites), and git.

```bash
npm ci                 # install
npm run dev            # run the desktop app with hot reload
npm run build          # build installers for this platform
```

The full check set, as CI runs it:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo test --workspace
npm run lint
npm run typecheck      # regenerates the ts-rs bindings first
npm test               # regenerates the ts-rs bindings first
```

The SVN integration suites, which need `svn` and `svnadmin` on `PATH`:

```bash
cargo test -p svnpush-core --features svn-integration --test svn_integration --test run_integration --test run_features
```

## Test results

The results below are from the final run on the commit this report
describes. Every suite passed.

| Suite | Tests | Result |
|---|---|---|
| `svnpush-core` unit tests | 186 | passed |
| `tests/ai_adapters.rs` (adapter fixtures) | 11 | passed |
| `tests/ai_client.rs` (mock-server lifecycle, retry, fallback) | 8 | passed |
| `tests/detect_fixtures.rs` | 10 | passed |
| `tests/package_fixtures.rs` | 5 | passed |
| `tests/roundtrip.rs` | 5 | passed |
| `tests/verify_checks.rs` | 32 | passed |
| `tests/timing.rs` | 1 | passed |
| `tests/svn_integration.rs` (`svn-integration` feature) | 7 | passed |
| `tests/run_integration.rs` (`svn-integration` feature) | 7 | passed |
| `svnpush-desktop` unit tests (services) | 19 | passed |
| **Rust total** | **291** | **0 failed, 0 ignored** |
| Frontend (vitest, 10 files) | 57 | passed |

`cargo fmt --check`, `cargo clippy` (pedantic, `-D warnings`), `cargo deny
check`, `npm run lint` and `npm run typecheck` were clean. `cargo deny`
prints one informational "unmatched license allowance" note.

## Decisions, summarised

[DECISIONS.md](DECISIONS.md) has 98 dated entries. By theme:

**Toolchain and repository.**
- Rust edition 2024 with `rust-version` 1.89, needed for `File::try_lock`,
  which backs the per-project lock. TypeScript 6.0, because
  `typescript-eslint` does not support 7 yet. Node 22 locally and in CI.
- A documented Clippy pedantic allow-list. `rustfmt` uses
  `use_small_heuristics = "Max"`.
- `cargo deny` checks `unmaintained` for direct dependencies only.
- ts-rs bindings are generated and git-ignored. App icons are committed.
  The tauri-generated permission files are no longer committed.
- App identifier `com.degird.svnpush`.
- Every crate added is justified per milestone:
  - M1: `thiserror`, `serde`, `regex`, `semver`, `ts-rs`
  - M2: `ignore`, `walkdir`, `blake3`, `zip`, `sha2`
  - M3: `tokio`, `tokio-util`, `roxmltree`
  - M4: `similar`, `keyring`, the dialog and opener plugins
  - M5: `reqwest`, `ulid`, `tracing`, `tracing-subscriber`,
    `tracing-appender`, the updater plugin
  - M6: `jsonschema`, and `wiremock` for tests

**Detection, readme and versions.**
- Errors reach the UI through one `Coded` trait as `{ code, message, fix }`.
- Edits are composed in an `EditSet` before anything is written.
- Readme headers are kept as an ordered, case-insensitive list with line
  numbers. Only the WordPress.org `=` heading syntax is parsed.
- Writing an entry for an existing version replaces its body.
- Empty version sources are reported, not skipped.
- Slugs come from the SVN URL.
- The real-world fixture copies AuthDock's readme and header block.
  Fixtures are byte-exact (`-text`).

**Packaging and checks.**
- Hard excludes match case-insensitively.
- Zips are deterministic: sorted entries, explicit directory entries, and a
  fixed timestamp. The `.sha256` file uses the `sha256sum` format.
- Check scope:
  - V12 defines "archive" and "executable" by extension.
  - V09 accepts an `ABSPATH` or `WPINC` guard.
  - A check that cannot run yet reports Skip with a reason.
  - W02 compares major.minor only.
- Paths over 260 characters produce a notice.
- Builds are pruned to three versions.

**Subversion.**
- The network timeout is Subversion's own `http-timeout=60`.
- Messages are forced to English without touching `LC_CTYPE`, and errors
  are classified by `E` codes.
- Every call uses `--no-auth-cache`, and the password goes over stdin.
- `svn add` runs with `--no-auto-props --no-ignore`, in batches of 100
  paths.
- An existing tag is refused, because a copy into it would nest.
- Tag verification makes three attempts over thirty seconds.
- A dry-run revert removes leftover unversioned files.
- The CI integration job runs on Linux and macOS.

**Run engine and desktop.**
- Detect, Write and the checks work on the project folder. Build packages
  the package root. V10–V12 wait for Build when a pre-build command
  creates that root.
- A dry run skips V16 and restores the snapshot.
- The change set comes from the last git tag plus untracked files, or from
  trunk. Paths are folder-relative inside larger repositories.
- The draft pre-fill uses a hand-bumped header version when there is one.
- Resume after the trunk commit creates only the tag. Resume before it
  rolls back and starts a fresh run of the same kind. Discard marks the
  journal.
- The UI gets the whole `RunState` on every change and keeps 2,000 log
  lines per project.
- Per-file SVN diffs are captured at Step 6, capped at 2 MB.
- Commands get `allow-*` permissions. Shell logic lives in testable
  `service/` functions.
- Real-window verification drives WebView2 through the debugging port with
  scratch `playwright-core`.

**Providers and AI.**
- M5 built the provider contract, adapters, client and records, because the
  Providers screen needs them. M6 added prompts, schemas, router and Draft
  wiring.
- Default models, checked on 17 September 2026:
  - Gemini: `gemini-3.8-flash`
  - Claude: `claude-sonnet-5`
  - OpenAI: `gpt-6-astra`
  - OpenRouter: `anthropic/claude-sonnet-5`
- Request formats:
  - Gemini uses `responseJsonSchema`.
  - OpenAI uses `max_completion_tokens`.
  - No temperature is sent to any provider.
  - OpenRouter gets the documented attribution headers.
- Client timeouts: 30 s for quick calls, 180 s for a synchronous answer.
- Revoye Test uses `/v1/status`.
- Log redaction covers every key shape.
- The explanation fix object carries `original` so a fix can be applied
  safely. Only a unique `readme.txt` replacement can be applied, and version
  text is never patched.
- The explanation starts automatically and waits in `AwaitingFixes`.
- Diff material limits: 20,000 characters per file, 60,000 in total, and
  up to 25 summarised files.
- Default exclude patterns are set, and secret-looking files are always
  withheld.
- The AI provider setting is machine-specific.
- The privacy notice is shown once per record, inline.
- The Change link choice lasts for the run.
- You can act while the AI works.
- Invalid or old versions are overridden to the next patch. Refusals and
  truncated answers are not retried.
- The release summary is shown but not written to any file.

**Accessibility, assets and distribution.**
- The assets-only mode runs V14, V16, W04 and V15, skips the other steps,
  and does not change "last release".
- Text meets AA contrast. Input borders meet 3:1. Phase changes are
  announced.
- Diagnostics export is Copy diagnostics.
- The build workflow uses `tauri-action@v1` with a draft release,
  `latest.json` (NSIS preferred) and signing secrets. Installers are
  unsigned for 1.0.
- README screenshots come from a demo plugin under `target/demo`.
- CI runs both integration suites.

## Known limitations

1. **No real WordPress.org release was made.** There is no committer account
   or throwaway plugin available to this build. Publishing was verified
   end to end against local `file://` repositories: in the window (M5), and
   in the engine integration suite, which covers publish, tag verification,
   resume, cancel, a dry run, an AI draft with an applied fix, and an
   assets-only commit.
2. **No real AI model produced a draft.** This machine has no Revoye,
   Gemini or other API key and no local model server. The adapters are
   tested against recorded vendor responses, the client and router against a
   mock server, and the whole flow in the real window against a local
   OpenAI-compatible stub. M6's "a real Revoye draft and a real Gemini draft
   succeed" remains for the owner to run.
3. **Only Windows was run by hand.** The Windows NSIS and MSI installers
   built locally in M0, and the keychain round trip was checked with Windows
   Credential Manager. The macOS and Linux installers and their keychains
   (Keychain, Secret Service) are exercised only by the CI workflows, which
   could not run from this machine.
4. **The updater key is a development key.** Its private key was generated
   for this build and is not in the repository. Before the first public
   release, the owner must generate their own pair, replace
   `plugins.updater.pubkey`, and add the signing secrets
   ([RELEASE.md](RELEASE.md)).
5. **Installers are not code-signed** (plan §19.1). Windows SmartScreen and
   macOS Gatekeeper warn on first launch.
6. **The M7 acceptance test is not done.** "Two external developers release
   a plugin with no help beyond the README" needs people outside this build.
7. Markdown-style (`##`) readmes are not parsed. Only the WordPress.org
   `=` heading syntax is.
8. When a diff is very large, at most 25 files are summarised before the
   draft; the rest are only named. The Revoye fleet line refreshes once a
   minute, and only while a job is waiting.
## What a reviewer should try first

1. **Setup.** Run `npm ci && npm run dev`. Open **Settings → Run Doctor** and
   confirm `svn`, `git` and the keychain are found.
2. **Rehearse against a local repository**, so nothing reaches WordPress.org:

   ```bash
   svnadmin create /tmp/svnpush-repo
   svn mkdir --parents -m "Seed" file:///tmp/svnpush-repo/my-plugin/trunk file:///tmp/svnpush-repo/my-plugin/tags file:///tmp/svnpush-repo/my-plugin/assets
   ```

   In **Vault**, add host `file`, any username, and any password. In
   **Projects → Add project**, choose a plugin folder and the URL
   `file:///tmp/svnpush-repo/my-plugin`.
3. **Dry run with no AI provider.** The Draft step shows "No AI provider
   configured" with the manual form. Approve, then read the checks, the
   file list and the SVN preview. Confirm your plugin files are unchanged
   afterwards.
4. **Break a check.** Make the readme's short description longer than 150
   characters and dry-run again. V07 fails and the release stops. With an AI
   provider set up, the explanation appears with a readme fix to review and
   apply.
5. **Add an AI provider** under **Providers**: Revoye, a Gemini key, or a
   local Ollama at `http://localhost:11434/v1`. Click **Test**, then release
   again. Check the one-time data notice, the draft, **Change**, and the
   stop button.
6. **Publish** to the local repository. Confirm the dialog, then check
   `svn ls file:///tmp/svnpush-repo/my-plugin/tags`. Cancel a second
   release while it waits at Publish, and confirm the files are restored.
7. **Update assets.** Add `.wordpress-org/banner-772x250.png` and click
   **Update assets**. Only `assets/` is committed.
8. **Secrets.** Search the logs under `<app-data>/svnpush/logs/` and the run
   journals for your passwords or keys. There should be no match. Confirm the
   entries exist in the OS keychain under service `svnpush`.
9. **Accessibility and themes.** Tab through a release and switch between
   Light and Dark.
