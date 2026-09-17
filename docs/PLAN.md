# SVNpush — Development Plan

> A desktop app that takes any WordPress plugin from a local folder to a
> published WordPress.org release in one click, safely, with AI doing the
> writing and rules doing the deciding.

Status: ready to start. Plan version 1.1, September 2026.

This document is the single source of truth for the first release of
SVNpush. Companion documents in this folder:

- `DEVELOPMENT-PROMPT.md` — the instruction that drives the build.
- `REVOYE-API.md` — the complete Revoye public API, the only reference the
  Revoye adapter may be written from.

Nothing in this plan depends on any particular plugin. It was designed
against real WordPress plugins and the WordPress.org SVN model, and its AI
layer mirrors the provider pattern already proven in the SyncDock client.

---

## 1. Purpose and principles

### 1.1 The problem

Releasing a WordPress.org plugin today means: bump the version in several
files, write the changelog, regenerate anything generated, build a package,
check nothing development-only leaked into it, check out an SVN working copy,
mirror the package into `trunk/`, add and delete the delta, commit, copy to
`tags/x.y.z`, commit again, and hope nothing was missed. GitHub Actions can
automate it but needs a repository, secrets, a tag push, and a workflow file
per plugin. Every developer solves it again for every plugin.

### 1.2 The goal

One install. One click per release. Any plugin. No GitHub dependency. The
release is faster than doing it by hand and safer than doing it by hand,
at the same time.

### 1.3 Principles (these decide every design question)

1. **AI proposes, rules decide, the developer approves.** AI writes the
   version bump, changelog and explanations. Deterministic checks decide if
   the release may proceed. A human clicks Publish. None of the three is
   ever skipped.
2. **Nothing irreversible without a preview.** Before any file in the plugin
   is changed and before any SVN commit, the exact change is shown.
3. **Defaults over configuration.** A new project needs a folder and an SVN
   URL. Everything else has a sensible default that can be changed later.
4. **Small surface.** Five screens. No plugin system, no marketplace, no
   accounts, no telemetry. Features that do not shorten or de-risk a release
   are not added.
5. **The engine is a library.** All logic lives in a core crate with no UI
   dependency, fully unit-tested, so the desktop app is a thin shell and a CLI
   can be added later at no cost.
6. **Fail loud, fail early, fail with a fix.** Every failed check names the
   file, the line where possible, and the fix. AI is used to explain, never to
   hide.
7. **The user chooses which AI runs, always.** Revoye is recommended and
   listed first. Nothing is ever silently substituted.

---

## 2. Scope

### 2.1 MVP (SVNpush 1.0)

- Add a plugin project from a local folder.
- Detect plugin metadata, versions, readme, package rules, git history.
- AI-drafted next version, changelog entry and upgrade notice, with manual
  editing and an "AI off" mode.
- AI providers through one adapter registry: **Revoye** (recommended, listed
  first), Google Gemini, Anthropic Claude, OpenAI, OpenRouter, DeepSeek, Qwen,
  Perplexity, any OpenAI-compatible endpoint, and local models (Ollama or
  LM Studio, no key). Adding a provider is one adapter file and one registry
  line.
- Write the approved version and changelog into the plugin files.
- Full verification checklist (section 5.4).
- Build the package (staged folder + zip + checksum) and show its file list.
- SVN: sparse working copy, trunk sync with add/modify/delete preview, assets
  sync, commit, server-side tag copy, post-publish verification.
- Dry run, cancel, rollback of local edits, resume after a partial publish.
- SVN credentials and AI keys in the OS keychain.
- Light and dark theme, system-following by default.
- Windows, macOS and Linux builds.

### 2.2 After MVP (in order)

1. Thin CLI (`svnpush release --dry-run`) over the same core, for CI use.
2. GitHub connection: read a repository as a project source, attach the zip
   to a GitHub release, push the release tag.
3. Optional WordPress Plugin Check integration when PHP and WP-CLI exist.
4. Team config sharing via a committed `.svnpush.json`, multi-account.
5. Themes (same SVN model, different readme rules).

### 2.3 Non-goals

- Not a code editor, not a git client, not a WordPress site manager.
- No hosted service, no login, no telemetry, no analytics.
- No bundled Subversion binary. The system `svn` is used and detected.
- No attempt to reimplement Plugin Check, PHPCS or PHPStan.
- No streaming AI output and no image generation. Every AI answer SVNpush
  needs is a JSON object, which is useless half-written.
- No AI cost accounting or pricing tables. A release makes two or three AI
  calls; a monthly request count per provider is enough.

---

## 3. Decisions

| Area | Decision | Why |
|---|---|---|
| Shell | **Tauri 2** | 5–10 MB installers, native dialogs, shell and keychain access, one codebase for three platforms. |
| Engine language | **Rust** (`svnpush-core` crate) | Single binary, no runtime to install, first-class crates for every job: `walkdir`, `ignore` (gitignore semantics for `.distignore`), `zip`, `regex`, `tokio::process`, `reqwest`, `keyring`, `semver`, `similar` (diffs), `blake3` (hashes), `jsonschema` (AI output validation). |
| UI | **React 19 + TypeScript + Vite** | Most widely known, strict typing, tiny app so bundle size is irrelevant. |
| Styling | **Plain CSS with design tokens**, no UI library, no Tailwind | Five screens do not justify a framework. Tokens make light/dark a one-line switch. |
| State | React state plus one small store (`zustand`) for the run | One long-running release run drives the whole UI; one store is enough. |
| SVN | **System `svn` CLI**, detected at startup | The only client safe to trust with a public release. |
| Git | Optional system `git` | Unlocks diff-based changelogs and clean-tree checks. Not required. |
| AI | **Adapter registry, Revoye first and recommended** | Ported from the SyncDock client's proven pattern (section 9). Every provider is a pure request-builder and response-parser; one HTTP client does all the fetching. |
| Secrets | **OS keychain** via the `keyring` crate | Windows Credential Manager, macOS Keychain, Linux Secret Service. Never a file. |
| Config | JSON in the app data directory, optional `.svnpush.json` in the plugin | No secrets in either. The in-repo file is auto-excluded from packages. |
| Packaging | `.distignore` if present, else a built-in default list | Matches the WP-CLI `dist-archive` convention developers already use. |
| App updates | Tauri updater, signed | Security fixes must reach users. |
| License | **MIT** (the repository already carries it) | Permissive, matches the tool being usable by any developer. |
| Language | English only in 1.0, every string in one file | Translation later is a file, not a refactor. |

### 3.1 Alternatives considered

| Choice | Alternatives | Why not |
|---|---|---|
| Tauri 2 | Electron; browser extension; Wails (Go) | Electron is ten times the size. An extension cannot run `svn` or read a folder tree. Wails has a smaller ecosystem and no updater story as mature. |
| Rust core | Node core inside Electron; Go | Node needs a runtime and cannot be a single binary. Go is fine but Tauri's shell is already Rust, so one language for the whole backend. |
| React | Svelte; SolidJS; vanilla | Svelte and Solid would be slightly smaller, but React has the widest contributor base and the best tooling for typed DTOs. |
| Plain CSS tokens | Tailwind; a component library | Both add a build dependency and a visual identity that is not ours, for five screens. |
| System `svn` | Bundled `svn`; WebDAV client in Rust; `git-svn` | Bundling means three platform builds of Subversion. A WebDAV client would be a from-scratch SVN protocol implementation. `git-svn` needs git and is slower. |
| Adapter registry | One provider only; an LLM gateway SDK | One provider locks users in. Gateway SDKs add a dependency and still need per-provider quirks handled. |
| OS keychain | Passphrase-encrypted file (as the browser extension does) | A desktop app has a real keychain; a passphrase vault is what you build when you do not. |

---

## 4. Architecture

```
┌──────────────────────────────────────────────────────────┐
│  Desktop UI (React)                                      │
│  Projects · Release · Providers · Vault · Settings       │
│  Renders the run state; never contains business logic    │
└───────────────▲───────────────────────────┬──────────────┘
                │ events (step progress)     │ commands (invoke)
┌───────────────┴───────────────────────────▼──────────────┐
│  Tauri shell (Rust, apps/desktop/src-tauri)              │
│  Thin command handlers · dialogs · window · updater      │
│  Maps core results and errors to serialisable DTOs       │
└───────────────────────────┬──────────────────────────────┘
                            │ plain Rust calls
┌───────────────────────────▼──────────────────────────────┐
│  svnpush-core (Rust crate, no Tauri dependency)          │
│                                                          │
│  detect      plugin header, readme, slug, versions       │
│  version     read · compare · write all version sources  │
│  readme      parse · validate · write changelog          │
│  package     exclusion rules · stage · zip · checksum    │
│  verify      the checklist (section 5.4)                 │
│  svn         checkout · sync · status · commit · tag     │
│  ai          provider contract · registry · order ·      │
│              router · client · adapters/* · prompts ·    │
│              schemas                                     │
│  vault       keyring wrapper                             │
│  run         release state machine · journal · rollback  │
│  tools       svn/git discovery and version gating        │
└──────────────────────────────────────────────────────────┘
```

Rules that keep this clean:

- `svnpush-core` never imports Tauri, never touches the window, never reads
  the keychain from inside a check. It receives what it needs as arguments.
- Every long operation is an `async fn` that takes a `Reporter` handle and
  emits typed progress events. The shell forwards them to the UI unchanged.
- Every error is a typed enum (`thiserror`) with a stable code, a message,
  and an optional `fix` string. The UI renders these; it never guesses.
- The UI holds one `RunState` object and renders it. Clicking a button sends
  one command. There is no UI-side workflow logic.
- `ai/client.rs` is the only place an AI HTTP call happens. Adapters never
  fetch.

---

## 5. The Release Protocol

This is the standard every release follows, in this order. The UI shows it
as a vertical checklist. Steps are numbered here exactly as they are shown.

### 5.1 Step 1 — Detect

Inputs: the project folder. Outputs: a `PluginFacts` record.

1. Find the main plugin file: the single `*.php` in the package root whose
   header block contains `Plugin Name:`. Zero or more than one is an error
   with a fix ("choose the main file in project settings").
2. Parse the header: Plugin Name, Version, Text Domain, Requires at least,
   Requires PHP, License.
3. Parse `readme.txt`: name line, Contributors, Tags, Requires at least,
   Tested up to, Requires PHP, Stable tag, License, short description,
   sections, changelog entries, upgrade notice entries.
4. Derive the slug from the SVN URL (authoritative), and compare with the
   folder name and Text Domain. Mismatches are warnings, except Text Domain,
   which WordPress.org requires to equal the slug (error).
5. Read `.distignore` and `.svnpush.json` if present.
6. If git exists: current branch, dirty files, last tag, commits since it.
7. Read the previous release: the trunk working copy if it exists, else the
   newest `tags/` entry listed from the server.

### 5.2 Step 2 — Changes and draft

1. Compute the change set for the AI:
   - With git: `git diff <last-tag>..HEAD --stat` plus the diff of source
     files, capped and prioritised (readme and PHP first, then JS/CSS, then
     the rest; generated, minified and vendored files excluded).
   - Without git: a file-level diff between the staged package and the trunk
     working copy (added, removed, modified with a compact text diff).
   - No changes at all: stop with "nothing to release".
2. Resolve the provider (section 9.5). Show which one will run, with a
   "Change" link. For Revoye, show the fleet hint (devices online, agents
   free, queue depth) before submitting.
3. Ask the provider for: next version with reason, changelog entry, upgrade
   notice, and a one-paragraph release summary. Strict JSON, validated
   against a schema.
4. Show the draft in an editor beside the change list. The developer can
   change the version, edit the text, regenerate, pick another provider, or
   switch to manual.
5. Approve. Nothing has been written to disk yet.

### 5.3 Step 3 — Write

1. Snapshot every file that will change (section 12.6).
2. Write the version to every source: plugin header, readme Stable tag,
   readme changelog (new entry inserted at the top of `== Changelog ==`),
   upgrade notice (if the section exists or the draft provides one), and each
   extra location configured for the project (a regex with one capture
   group, for example a `define( 'MYPLUGIN_VERSION', '…' )` constant, a
   `package.json` or `composer.json` version).
3. Show the unified diff of every written file. This diff is also stored in
   the run journal.

### 5.4 Step 4 — Verify (the gate)

All checks run. All blocking checks must pass. Warnings are shown but do not
block. The developer cannot disable a blocking check from the UI; a project
can add checks, never remove them.

Blocking checks:

| ID | Check | Fix shown |
|---|---|---|
| V01 | Every version source holds the same value | List of sources and values |
| V02 | Version is valid (`x.y` or `x.y.z`, optional pre-release suffix) | Expected pattern |
| V03 | Version is greater than the previous release and no `tags/<version>` exists on the server | Suggest next version |
| V04 | Readme changelog names this version as its newest entry | Insert entry |
| V05 | Readme required headers present: name line, Contributors, Tags, Requires at least, Tested up to, Requires PHP, Stable tag, License, License URI | Missing header names |
| V06 | Stable tag equals the version being released and is not `trunk` | Set stable tag |
| V07 | Short description is 150 characters or fewer | Character count |
| V08 | Text Domain equals slug | Set text domain |
| V09 | Main plugin file has an `ABSPATH` (or equivalent) guard | Snippet |
| V10 | Package contains the main file, `readme.txt`, and every path the project marks required | Missing paths |
| V11 | Package contains no forbidden path (section 7.3) | Offending paths |
| V12 | Package contains no archive, executable, or VCS directory (`.zip`, `.exe`, `.dll`, `.phar` unless allowed, `.git`, `.svn`) | Offending paths |
| V13 | Zip entries use `/` separators and all sit under `<slug>/` | Internal |
| V14 | Subversion available and at least 1.10 | Install instructions |
| V15 | SVN working copy has no conflicts and is up to date with the server | Resolve or reset |
| V16 | Credentials exist in the vault for this SVN account | Open vault |

Warnings:

| ID | Check |
|---|---|
| W01 | Git working tree has uncommitted changes (the release will not match any commit) |
| W02 | `Tested up to` is older than the current WordPress release (fetched from api.wordpress.org, optional and cached for a day) |
| W03 | Tags count exceeds five |
| W04 | Screenshots referenced in readme with no matching `screenshot-N.*` in the assets folder, or the reverse |
| W05 | Package larger than 10 MB, or any single file over 2 MB |
| W06 | Pre-release version suffix (WordPress.org serves it if the stable tag points to it) |
| W07 | `Requires PHP` or `Requires at least` in readme differs from the plugin header |
| W08 | A file in the package is ignored by `.gitignore` (probably build output that belongs in `.distignore`) |
| W09 | Line endings mixed in `readme.txt`, or a UTF-8 BOM present |
| W10 | `vendor/` present and larger than 5 MB |

After the checks, if anything failed, the provider is asked to explain the
failures in plain language and suggest the smallest fix. For readme-only
fixes the suggestion can be applied with one click; it then goes through
Step 3's diff preview again. Code fixes are never applied by the app.

### 5.5 Step 5 — Build

1. Run the project's pre-build hook if configured (for example
   `npm run build` or `composer install --no-dev`). The command is shown
   before it runs, output is streamed, a non-zero exit stops the release.
2. Stage: walk the package root, apply exclusion rules (section 7), copy to
   `<app-data>/builds/<slug>/<version>/<slug>/`.
3. Re-run V10 to V13 against the staged tree, not the source tree.
4. Zip with forward-slash entries, sorted, `<slug>/` prefix, deterministic
   order. Write `<slug>-<version>.zip` and `<slug>-<version>.zip.sha256`.
5. Show the file list with sizes, and the total.

### 5.6 Step 6 — Preview SVN

1. Ensure the sparse working copy (section 8.2). `svn update` it.
2. Mirror the staged tree into `trunk/`: copy new and changed files (compared
   by content hash, not timestamp), delete files in trunk that are absent
   from the staged tree, never touch `.svn`.
3. `svn add --force`, `svn delete` for missing, `svn propset svn:mime-type`
   on images and other binaries.
4. If an assets folder is configured (default `.wordpress-org/` if present):
   mirror it into `assets/` the same way.
5. Show `svn status` grouped as Added / Modified / Deleted for trunk and for
   assets, with a per-file diff on click. Show the commit messages that will
   be used. Show the tag path that will be created.

In dry-run mode the run ends here with a full report and no commit. The
working copy is reverted (`svn revert -R`) so the next run starts clean.

### 5.7 Step 7 — Publish

1. Second confirmation: version, slug, SVN URL, account, and the counts from
   the preview. Typing is not required; a single explicit button is.
2. Unlock the vault (OS prompt if the keychain requires one).
3. `svn commit trunk assets -m "Release <version>"` with credentials over
   stdin (section 8.4). Record the revision number.
4. `svn copy <URL>/trunk <URL>/tags/<version> -m "Tag <version>"` as a
   server-side copy. Record the revision number.
5. Verify: `svn ls <URL>/tags/<version>/` returns the main file, and
   `svn cat <URL>/tags/<version>/readme.txt` carries the right stable tag.
6. Optional post-publish actions, each opt-in per project: create a local git
   tag `v<version>` and a commit "Release <version>"; open the plugin page
   in the browser; copy the zip path to the clipboard.
7. Write the run journal as complete.

### 5.8 Timing target

From Approve (end of Step 2) to a verified tag: under two minutes on a
normal connection for a mid-sized plugin, most of it network. Detection and
verification must complete in under two seconds for a 5,000-file tree. The
AI draft itself is excluded from the target because it depends on the
provider: a model API answers in seconds, a Revoye job takes 20 to 90
seconds by design, and the UI says so while it waits.

---

## 6. Project detection and configuration

### 6.1 Adding a project

Native folder dialog, then detect, then ask only for the SVN URL. The slug
is derived from it, and `https://plugins.svn.wordpress.org/<slug>` is offered
pre-filled from the folder name. Save.

### 6.2 Project settings (all optional, all with defaults)

| Setting | Default |
|---|---|
| Package root | project folder (can be a `dist/` or `build/` subfolder) |
| Main plugin file | auto-detected |
| Extra version locations | none (each: relative file path + regex with one capture group) |
| Required paths | main file, `readme.txt` |
| Pre-build command | none |
| Assets folder | `.wordpress-org/` if it exists, else none |
| Exclusion source | `.distignore` if present, else built-in defaults |
| AI provider | the default provider record; can be pinned to one, or "off" |
| AI exclude patterns | vendor, minified, generated, `*.lock` |
| Post-publish: git tag and commit | off |
| Post-publish: open plugin page | on |
| SVN account | the single vault account, or a chosen one |

### 6.3 Storage

- `<app-data>/svnpush/projects.json` — the list of projects and settings,
  keyed by absolute path. No secrets. Carries `"schema": 1`.
- `<app-data>/svnpush/providers.json` — AI provider records (section 9.6).
  No keys.
- `<project>/.svnpush.json` — optional, same shape as a project entry minus
  machine-specific fields, for teams. Auto-excluded from packages. Values
  here override the app-level entry for that project. Carries `"schema": 1`.
- `<app-data>/svnpush/wc/<slug>/` — the sparse SVN working copy.
- `<app-data>/svnpush/builds/<slug>/<version>/` — staged trees and zips,
  pruned to the last three versions.
- `<app-data>/svnpush/runs/<slug>/<timestamp>.json` — run journals.
- `<app-data>/svnpush/logs/` — rotated, redacted logs; "Copy diagnostics"
  in Settings bundles the last log and the Doctor report for a bug report.

---

## 7. Packaging rules

### 7.1 Exclusion sources, in priority order

1. `.distignore` in the package root (gitignore syntax via the `ignore`
   crate: leading `/` anchors, bare names match at any depth, `!` negates).
2. If absent, the built-in default list (7.2).
3. Always, regardless of the above: the hard-excluded list (7.3).

### 7.2 Built-in defaults (used only when no `.distignore` exists)

```
.git .github .gitignore .gitattributes .gitlab-ci.yml
.svn .svnpush.json .distignore .editorconfig
node_modules tests test phpunit.xml phpunit.xml.dist
phpcs.xml phpcs.xml.dist phpstan.neon phpstan.neon.dist
composer.json composer.lock package.json package-lock.json yarn.lock
webpack.config.js vite.config.* tsconfig.json .babelrc .eslintrc* .prettierrc*
docker-compose.yml Dockerfile .docker .wordpress-org
README.md CHANGELOG.md CONTRIBUTING.md SECURITY.md
.DS_Store Thumbs.db *.log *.zip *.map .phpunit.result.cache
/release /build /dist /coverage /.idea /.vscode *.swp
```

`/vendor` is deliberately not in the defaults: many plugins ship Composer
dependencies. W10 warns when it is large so the developer notices.

### 7.3 Hard-excluded, always

`.git`, `.svn`, `.hg`, `.svnpush.json`, `.distignore`, `*.zip`, `*.tar.gz`,
`.DS_Store`, `Thumbs.db`, and the app's own build output directory.

### 7.4 Staging semantics

- Symlinks are followed once and copied as files; a symlink loop is an error.
- File names are validated as UTF-8 and free of characters Windows cannot
  write; violations are errors with the path.
- Empty directories are not packaged.
- Content hashes (BLAKE3) are computed during staging and reused by the SVN
  sync, so nothing is read twice.

---

## 8. SVN engine

### 8.1 Discovery

At startup and on demand (Settings → Doctor): locate `svn` on PATH, at the
configured path, or at the usual install locations (Windows:
`C:\Program Files\TortoiseSVN\bin\svn.exe`, `C:\Program Files\SlikSvn\bin\svn.exe`;
macOS: `/opt/homebrew/bin/svn`, `/usr/local/bin/svn`). Parse
`svn --version --quiet`, require 1.10 or newer for `--password-from-stdin`.
If missing, show platform instructions:

- Windows: TortoiseSVN with command-line tools, `choco install svn`, or
  SlikSVN.
- macOS: `brew install subversion` (Xcode no longer ships it).
- Linux: `apt install subversion` or the distribution's equivalent.

### 8.2 Sparse working copy

WordPress.org repositories hold every tag ever released, so a full checkout
can be gigabytes. SVNpush checks out:

```
svn checkout --depth immediates <URL> wc/<slug>
svn update --set-depth infinity wc/<slug>/trunk
svn update --set-depth infinity wc/<slug>/assets
```

`tags/` and `branches/` stay at depth empty. Tags are created server-side
(`svn copy URL URL`), so they never need to exist locally. The working copy
is reused across releases and repaired (`svn cleanup`, then re-checkout if
that fails) rather than deleted.

### 8.3 Sync algorithm (staged tree → trunk)

1. Build a map of relative path → hash for the staged tree (from staging).
2. Walk `trunk/` excluding `.svn`; hash each file.
3. For each staged file: absent in trunk → copy and `svn add`; different hash
   → copy; identical → skip.
4. For each trunk file absent from staged → `svn delete`.
5. Directories that become empty are deleted by `svn delete` as well.
6. Set `svn:mime-type` for `png`, `jpg`, `jpeg`, `gif`, `webp`, `svg`, `ico`,
   `woff`, `woff2`, `ttf`, `eot`, `pdf`, `mp3`, `mp4`. Never set
   `svn:eol-style`, so line endings reach WordPress.org unchanged.
7. The same routine runs for the assets folder → `assets/`.

### 8.4 Credentials at the command line

Every `svn` invocation that talks to the server runs with:

```
--non-interactive --no-auth-cache --username <user> --password-from-stdin
```

The password is written to the child's stdin and the handle closed. It never
appears in the process list or on disk. Output is captured and any line
containing the password is redacted before logging (defensive; it should
never appear).

### 8.5 Commit messages

Defaults, editable in the preview:

- Trunk: `Release <version>`
- Tag: `Tag <version>`
- Assets-only run: `Update listing assets`

### 8.6 Post-publish verification and resume

`svn ls` and `svn cat` against the tag URL as described in 5.7. If the trunk
commit succeeded but the tag copy failed (network drop, server hiccup), the
journal records the trunk revision and the UI offers **Resume: create tag**,
which retries only the copy.

### 8.7 Assets-only releases

A separate small action on the project page: sync and commit `assets/` alone,
no version change. Uses the same preview and confirm flow.

---

## 9. AI layer

The AI layer is a port of the SyncDock client's provider system
(`shared/js/services/ai/`) to Rust. The shape, the names, the error codes
and the Providers screen are kept the same so that anyone who knows one
knows the other. Three things are deliberately left out because SVNpush
does not need them: streaming, image generation, and USD cost accounting.

### 9.1 Provider contract (`ai/provider.rs`)

Every adapter is a **pure request-builder and response-parser**. Adapters
never perform network calls. `ai/client.rs` is the only place a request is
sent and the only place a key exists in plaintext for the duration of a
call.

```
page → run::draft() → ai::router::generate()
     → adapter.build_request(key) → client.fetch → adapter.parse_response()
     → (pending? client polls with adapter.poll) → normalised GenerateResult
```

```rust
pub struct AdapterMeta {
    pub kind: &'static str,            // registry key: "revoye" | "gemini" | "claude" | "openai" | …
    pub label: &'static str,           // human name shown in the UI
    pub recommended: bool,             // true only for Revoye; shows "(Recommended)" in the picker
    pub default_base_url: &'static str,
    pub fixed_base_url: bool,          // true hides the Base URL field (first-party endpoints)
    pub default_model: &'static str,
    pub models: &'static [&'static str], // closed list → dropdown; empty → free text
    pub keyless: bool,                 // local models need no key
    pub key_placeholder: &'static str,
    pub note: &'static str,            // one or two sentences under the type picker
    pub model_hint: &'static str,      // one line under the model field
}

pub struct GenerateRequest<'a> {
    pub base_url: Option<&'a str>,
    pub api_key: Option<&'a str>,
    pub model: &'a str,
    pub system: Option<&'a str>,
    pub messages: &'a [Message],       // { role: User | Assistant, content: String }
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub json_schema: Option<&'a serde_json::Value>,
}

pub struct HttpRequest { pub method: Method, pub url: String, pub headers: Vec<(String, String)>, pub body: Option<serde_json::Value> }

pub enum ParseOutcome {
    Complete(GenerateResult),
    Pending { job_id: String, status: String, queue_position: Option<u32> },
}

pub struct GenerateResult {
    pub text: String,
    pub tokens_in: Option<u32>,        // None (never 0) when the provider reports nothing
    pub tokens_out: Option<u32>,
    pub finish_reason: FinishReason,   // Stop | Length | Safety | Other(String)
    pub raw: serde_json::Value,
}

pub trait Adapter: Send + Sync {
    fn meta(&self) -> &AdapterMeta;
    fn build_request(&self, req: &GenerateRequest) -> Result<HttpRequest, ProviderError>;
    fn parse_response(&self, json: &serde_json::Value) -> Result<ParseOutcome, ProviderError>;
    fn parse_error(&self, status: u16, json: &serde_json::Value) -> ProviderError;
    fn list_models(&self) -> Option<&dyn ListModels> { None }
    fn poll(&self) -> Option<&dyn Poll> { None }
    fn fleet_status(&self) -> Option<&dyn FleetStatus> { None }
}

pub trait ListModels { fn build_request(&self, base_url: Option<&str>, api_key: Option<&str>) -> HttpRequest;
                       fn parse_response(&self, json: &serde_json::Value) -> ModelList; } // { models, note }
pub trait Poll       { fn interval_ms(&self) -> u64; fn max_ms(&self) -> u64;
                       fn build_request(&self, api_key: &str, job_id: &str) -> HttpRequest;
                       fn parse_response(&self, json: &serde_json::Value) -> Result<ParseOutcome, ProviderError>;
                       fn build_cancel(&self, api_key: &str, job_id: &str) -> HttpRequest; }
pub trait FleetStatus{ fn build_request(&self, api_key: &str) -> HttpRequest;
                       fn parse_response(&self, json: &serde_json::Value) -> Fleet; }
```

Normalised failure, identical to the SyncDock codes so the UI copy and the
fallback rules carry over unchanged:

```rust
pub enum ErrorCode {
    Auth,          // invalid, expired or revoked key (401/403)
    RateLimit,     // 429, retry_after seconds when known
    Server,        // provider 5xx, overloaded, fleet not available
    Payment,       // out of credits (402)
    Truncated,     // finish reason "length", content may be partial
    Safety,        // the model refused
    ModelNotFound, // unknown or retired model (404)
    Network,       // request failed, local server not running
    BadRequest,    // 400, malformed request or oversized prompt
    NoProvider,    // nothing configured
}

pub struct ProviderError { pub code: ErrorCode, pub status: u16, pub retry_after: Option<u32>,
                           pub provider_kind: Option<&'static str>, pub message: String }

pub fn error_from_status(status: u16, message: String, kind: &'static str) -> ProviderError;
pub fn trim_base(url: &str) -> String;
```

### 9.2 Registry and order (`ai/registry.rs`, `ai/order.rs`)

- `registry::get(kind) -> Result<&'static dyn Adapter, ProviderError>`
  (`NoProvider` with a clear message when unknown).
- `registry::list() -> Vec<&'static dyn Adapter>` returns every adapter with
  **Revoye first**; everything else keeps registration order.
- `order::FIRST_PARTY_KIND = "revoye"` and `order::revoye_first(list)` reorder
  any list of things with a `kind`. Display only: which provider runs is
  decided by the router, never by this order.
- Adding a provider is one file under `ai/adapters/` and one line in the
  registry. Nothing else changes.

Registered adapters, in this order after Revoye:

| Kind | Label | Wire dialect | Default model | Notes |
|---|---|---|---|---|
| `revoye` | Revoye | Revoye `/v1/completions` + poll | `revoye/auto` | **Recommended.** Fixed base URL. Models: `revoye/auto`, `chatgpt`, `claude`, `gemini`, `deepseek`, `qwen`, `perplexity`. |
| `gemini` | Google Gemini | Generative Language API `generateContent` | current Flash-class id | `x-goog-api-key` header; JSON via `responseMimeType` + `responseSchema`. |
| `claude` | Anthropic Claude | Messages API `/v1/messages` | current Sonnet-class id | `x-api-key` + `anthropic-version`; JSON via `output_config.format.json_schema`; no sampling params sent. |
| `openai` | OpenAI | `/chat/completions` | current GPT id | via the OpenAI-compatible factory; JSON via `response_format: json_object`. |
| `openrouter` | OpenRouter | `/chat/completions` | `anthropic/claude-sonnet-5` | Factory plus attribution headers `HTTP-Referer` and `X-OpenRouter-Title`, 200-with-error handling, live `GET /models` catalogue. |
| `deepseek` | DeepSeek | `/chat/completions` | `deepseek-chat` | Factory. |
| `qwen` | Qwen (DashScope) | `/chat/completions` | `qwen-plus` | Factory. |
| `perplexity` | Perplexity | `/chat/completions` | `sonar-pro` | Factory. |
| `openai_compatible` | OpenAI-compatible | `/chat/completions` | free text | Factory; base URL required. |
| `local` | Local model (Ollama / LM Studio) | `/chat/completions` | `gemma3` | Factory; keyless; default base URL `http://localhost:11434/v1`. |

`ai/adapters/openai.rs` exposes `OpenAiCompatible::new(spec)`, the factory
behind every `/chat/completions` provider, exactly as `makeOpenAiCompatible`
does in SyncDock. Default base URLs and model ids are starting points held on
the provider record, not hardcoded truths, so a retired model is a settings
edit, not a release.

### 9.3 The Revoye adapter (`ai/adapters/revoye.rs`)

Written only from `docs/REVOYE-API.md`. The public API is six endpoints and
nothing outside it exists. Behaviour, mirroring the SyncDock adapter:

- **Fixed base URL** `https://api.revoye.com`; `fixed_base_url: true`, so the
  Providers form hides the field. `recommended: true`.
- **Model is a provider constraint**, not a model id: `revoye/auto` lets the
  account's rotation choose; a kind such as `chatgpt` pins it and is never
  silently answered by another. An unknown value is `BadRequest`.
- **Prompt flattening.** System text first, then each turn labelled `User:`
  or `Assistant:` (a lone user turn is unlabelled), joined by blank lines.
  Revoye has no structured-output mode, so when a JSON schema is requested
  the prompt ends with an unambiguous instruction to reply with a single
  JSON object matching the schema and nothing else. Empty prompt or more
  than 100,000 characters is `BadRequest` before any call.
- **Submit with `wait: false`**, then poll. `timeout_ms: 180000`,
  `deadline_ms: 600000`, `metadata: { "source": "svnpush", "task": <task> }`.
  The job is durable the moment it is accepted, so a cancelled poll loop
  never loses work.
- **Idempotency-Key** derived from the work, not random:
  `svnpush:<slug>:<version>:<task>:<blake3(prompt)[..16]>`. A crash and
  retry recomputes the same key and gets the same job back instead of
  running a second one. This is the single deliberate difference from the
  SyncDock adapter, which cannot retry and therefore uses a random UUID.
- **Poll** `GET /v1/completions/{id}` every 4 s, up to 600 s, showing
  `queue_position` as "waiting for an agent (N ahead)" in the UI. Cancel via
  `DELETE /v1/completions/{id}` when the user cancels the step, because
  aborting the HTTP request alone does not stop the job.
- **Result parsing.** `succeeded` → text; `failed` → `Server` with the attempt
  count; `expired` → `Server`; `cancelled` → `Server`; `succeeded` with
  `response: null` and `content_pruned_at` set → `Server` with a message that
  the text was removed by retention, run again.
- **Tokens** are always `None`. Usage is counted as requests.
- **Error envelope** branches on `error.code`, never on `message`:
  `UNAUTHORIZED` → `Auth`; `FORBIDDEN` with `details.limit` → `RateLimit`
  (queue full), otherwise `Auth` (missing scope); `RATE_LIMITED` →
  `RateLimit` with `retry_after`; `PROVIDER_RATE_LIMITED` → `RateLimit`;
  `NO_DEVICE_ONLINE`, `NO_AGENT_AVAILABLE`, `JOB_TIMEOUT`, `JOB_FAILED`,
  `JOB_CANCELLED`, `NOT_FOUND`, `INTERNAL_ERROR` → `Server` with the
  matching plain-language message; `PAYLOAD_TOO_LARGE`, `CONFLICT`,
  `INVALID_REQUEST` → `BadRequest`; unknown code → `error_from_status`.
- **`list_models`** `GET /v1/models`: the providers enabled on this account,
  `revoye/auto` first. An empty list with `_revoye.note` is "nothing
  configured", shown as the note, not as a broken endpoint.
- **`fleet_status`** `GET /v1/status`: devices, agents, per-provider idle
  agents, queue depth. Shown as one line in the Providers form and in the
  Draft step. Never polled in a loop; a snapshot, not a lock.
- **Never logged:** the key, the prompt, the response. Logged: `error.code`,
  HTTP status, `X-Request-Id`.

### 9.4 HTTP client and polling (`ai/client.rs`)

- `reqwest` with explicit timeouts: 30 s for submit, list, status, poll and
  cancel calls; the adapter's `max_ms` bounds the whole poll loop.
- Non-2xx → `adapter.parse_error(status, body)`. 2xx →
  `adapter.parse_response(body)`. `Pending` → sleep `interval_ms`, poll,
  repeat until `Complete`, an error, `max_ms`, or cancellation.
- A `CancellationToken` from the run cancels the loop and sends
  `build_cancel` when the adapter has one.
- `RateLimit` with `retry_after` on a poll waits that long once, then
  continues. Submits are never retried by the client; the router decides.
- The key is read from the keychain immediately before the call and dropped
  immediately after. A `tracing` layer redacts anything that looks like a
  key in the log.

### 9.5 Router (`ai/router.rs`)

Resolves which provider record runs a generation. The developer's explicit
choice always wins; automatic resolution only fills gaps:

1. Per-run override (the "Change" link in the Draft step).
2. The project's pinned provider, if set.
3. The record marked default.
4. The only record, if there is exactly one.
5. Otherwise `NoProvider`: "No AI provider configured. Add one on the
   Providers screen; Revoye is recommended, and a local model works without
   a key."

**Fallback is opt-in.** The Providers screen has an ordered fallback list,
off by default. When on, a failure with code `Auth`, `Payment`, `RateLimit`,
`Server`, `Network` or `ModelNotFound` rolls over to the next provider in the
user's order, and the result is marked `fell_back_from`. A `Safety`,
`BadRequest` or `Truncated` failure never rolls over: a different AI does not
fix a refused or malformed request. When the chain is empty or exhausted the
original error surfaces. The UI always says which provider produced a
draft.

### 9.6 Provider records and keys

```
ProviderRecord { id: "prov_<ulid>", kind, label, model, base_url: Option<String>,
                 has_key: bool, is_default: bool, needs_attention: Option<String>,
                 created_at, requests_this_month: u32 }
```

Stored in `providers.json`. The key lives in the OS keychain under service
`svnpush`, account `ai:<id>`, and is written only from the Providers form.
Removing a record removes its keychain entry. `needs_attention` is set by
the router on an `Auth` or `Payment` failure and cleared by the user or by a
successful test.

### 9.7 Prompts and schemas (`ai/prompts.rs`, `ai/schemas.rs`)

Three tasks, each with a system prompt, a user prompt builder and a JSON
schema:

- **`draft_release`** receives, in this order: plugin name and slug,
  previous version, the existing changelog's last three entries (for style),
  the commit subjects since the last release, the diff stat, then the diff
  itself up to a character budget (default 60,000; larger diffs are
  summarised per file first with `summarise_file`, then combined).
  Instructions: match the plugin's existing changelog tone, user-facing
  language, group by Added / Changed / Fixed / Security when the plugin
  already does so, no marketing, no invented features, suggest the version
  by semantic versioning and explain the reason in one sentence. Output:
  `{ version, reason, changelog_markdown, upgrade_notice, summary }`.
- **`summarise_file`** receives one file's diff and returns
  `{ path, summary }`. Used only when the diff exceeds the budget.
- **`explain_failures`** receives the check IDs, messages and the relevant
  file excerpts and returns `{ explanation, fixes: [{ check_id, path,
  replacement }] }` where `replacement` is present only for readme and
  version-text fixes.

Structured output per dialect: Gemini `responseMimeType: application/json`
plus `responseSchema`; Claude `output_config.format.json_schema`; the
OpenAI family `response_format: json_object` with the schema also stated in
the prompt; Revoye by instruction in the prompt. Every answer is validated
with the `jsonschema` crate regardless of dialect. A validation failure
triggers one retry with the validation error appended to the prompt, then
the step falls back to manual with the raw text shown.

### 9.8 Privacy and control

- First use of AI shows a one-time notice: which data leaves the machine
  (diff excerpts, readme, commit subjects) and to which provider. Revoye's
  note adds that answers come from AI accounts the developer is already
  signed into, through Revoye Desk on their own machine.
- Exclude patterns per project keep vendored and sensitive files out of the
  prompt. Files that look like secrets (`.env`, `*.pem`, `*.key`,
  `wp-config.php`) are always excluded.
- **AI off** mode: Step 2 becomes a manual version and changelog form. All
  other steps are identical. The safety of a release never depends on AI.
- The AI never runs commands, never writes files directly, never sees
  credentials. Its output is text that the developer approves.
- Model output is untrusted text: rendered as text, never interpreted,
  never passed to a shell.

---

## 10. Security and privacy

- Secrets: OS keychain only, under service name `svnpush`; SVN accounts as
  `svn:<host>:<username>`, AI keys as `ai:<provider-id>`.
- No secret is ever written to a log, a journal, a config file or the UI.
- Network calls: WordPress.org SVN, the configured AI provider, and
  (optional, opt-out) `api.wordpress.org` for the current WordPress version.
  Nothing else. No telemetry of any kind.
- Content Security Policy in the webview: no remote scripts, no eval.
- Tauri capability files grant only the commands the UI needs. Shell access
  is limited to the `svn` and `git` binaries, plus the user-configured
  pre-build hook, which is displayed before every run.
- App updates are signed with the Tauri updater key; the public key is
  compiled in.
- A lock file per project prevents two concurrent releases of the same plugin,
  including from two app instances.
- WordPress.org reminder shown in the Vault: the SVN password is the
  application-specific password generated on the WordPress.org profile page,
  not the account password (required since two-factor became mandatory for
  committers).

---

## 11. UI and UX

### 11.1 Screens

1. **Projects** — a list: name, slug, current version, last release date and
   result. "Add project" button. Click a project to open it.
2. **Project / Release** — header (name, version, SVN URL, account, AI
   provider), a primary **Release** button and a **Dry run** toggle, then the
   seven-step checklist. Each step is a card that expands to its content:
   the AI draft editor, the diff, the check table, the file list, the SVN
   preview, the publish confirmation. A collapsible log drawer at the bottom
   streams command output. Past releases in a small table below.
3. **Providers** — the AI provider screen, a direct port of the SyncDock
   Providers page:
   - Title "AI Providers", subtitle "Connect the AI that writes your release
     notes. You choose which runs, always." Primary button "Add provider".
   - List, Revoye first: icon, label, "Default" badge, "kind · model" meta,
     a "Needs attention" line when set, monthly request count, and actions
     Test / Set as default / Edit / Remove. An attention row also shows
     Clear.
   - Empty state: "No AI providers yet. Revoye is recommended. A local model
     works without an API key." with "Add provider".
   - Add / Edit form: Provider (select, Revoye listed first with
     "(Recommended)"), Label, Model (a dropdown when the adapter has a closed
     list or `list_models`, with a ↻ button to load the account's own list;
     free text otherwise; the adapter's `model_hint` beneath; for Revoye a
     fleet line beneath that), API key (password field, write-only, with
     show/hide; placeholder "Unchanged — paste a new key to replace it" when
     editing; hint "Stored in your operating system keychain. Never written
     to a file."; hidden for keyless adapters). Advanced (collapsed): Base
     URL (hidden when `fixed_base_url`), Default provider toggle. Test result
     area. Save.
   - Automatic fallback (collapsed, opt-in): the explanation that SVNpush
     normally tells you rather than quietly using a different AI, an ordered
     checklist of providers, Save.
   - The adapter's `note` appears under the Provider select when a type is
     chosen. Pasting a key auto-loads the model list silently.
4. **Vault** — SVN accounts (host, username, write-only password field) with
   Test. The WordPress.org SVN-password reminder.
5. **Settings** — theme (System / Light / Dark), `svn` and `git` paths with
   Doctor results, privacy toggles (WordPress version lookup), Copy
   diagnostics, app version and update check.

### 11.2 Design rules

- Design tokens on `:root`, dark values under `[data-theme="dark"]` and
  under `prefers-color-scheme: dark` when the theme is System. The toggle
  is a three-way segmented control in the title bar; the choice persists.
- One accent colour, neutral greys, semantic green/amber/red for check
  states. WCAG AA contrast in both themes.
- Every action is keyboard reachable. Focus rings are visible. The
  reduced-motion media query is respected.
- Monospace for paths, versions, commands and diffs. Prose everywhere else.
- Empty states say what to do next. Error states show the fix.
- No modal dialogs except the final Publish confirmation and destructive
  actions (remove project, remove provider, forget account).
- Window minimum 960×640; the layout is a single column with the log
  drawer, so it never needs a second breakpoint.
- All user-visible strings live in one `strings.ts` file.

### 11.3 Copy

Short, factual, in the second person. Buttons are verbs: Release, Dry run,
Approve, Publish, Resume, Cancel, Add provider, Test. Never "Oops". Never an
exclamation mark.

---

## 12. Edge cases and failure handling

### 12.1 Project shapes

- **First release**: `trunk/` empty on the server. Sync adds everything; the
  previous version is "none"; AI drafts a first changelog entry.
- **Existing manual history**: tags exist that were never made by SVNpush.
  V03 lists them from the server; the previous version is the newest tag.
- **Package root is a subfolder** (`dist/`, `build/`): set in settings; the
  pre-build hook produces it; git diff still runs at the project root.
- **Composer plugins**: pre-build `composer install --no-dev
  --optimize-autoloader`, `vendor/` ships, `.distignore` keeps dev files out.
- **Block plugins with a build step**: pre-build `npm ci && npm run build`;
  `src/` excluded, `build/` included, by the plugin's own `.distignore`.
- **Multiple `Plugin Name:` files** (a bundled library or a demo): error
  with a picker; the choice is saved.
- **Stable tag: trunk**: V06 fails with an explanation and the one-click fix.
- **Version like `1.0`** versus a tag `1.0.0`: compared with semver
  normalisation; the tag folder uses the version exactly as written in the
  header.
- **Pre-release versions** (`1.2.0-beta1`): allowed, W06 warns, WordPress.org
  will serve it only if the stable tag points to it.
- **Monorepo**: each plugin is its own project with its own package root.

### 12.2 Environment

- `svn` missing or too old: V14 with instructions; nothing else runs.
- `git` missing: no diff-based draft; the trunk-comparison fallback is used
  silently; W01 is skipped.
- Keychain unavailable (some Linux setups without Secret Service): the
  Vault and Providers screens explain it and offer no fallback to plain
  files. Everything except Publish and AI still works.
- No network: Detect and Verify work from the local working copy; Preview
  and Publish fail fast with a network error, not a hang. Every `svn`
  network call has a 60-second timeout and can be cancelled.
- Windows long paths: enabled via the manifest; paths over 260 characters
  still warn because WordPress.org's own tooling may reject them.
- Case-only renames: detected explicitly (trunk `Foo.php` versus staged
  `foo.php`) and performed as delete plus add.

### 12.3 SVN server

- 401: "credentials rejected" with the SVN-password reminder.
- 403 on commit: the account is not a committer for this slug; show the
  slug and the username.
- Conflict on update: V15 fails; the fix offered is "Reset working copy",
  which re-checks-out the sparse tree.
- Tag already exists: V03 fails before anything is written.
- Trunk committed, tag failed: Resume (8.6).
- Server-side copy succeeds but `svn ls` verification fails (replication
  lag): retry three times over thirty seconds, then mark "published,
  unverified" and tell the user to check the plugin page.

### 12.4 AI

- No provider configured: the Draft step shows the `NoProvider` message with
  a link to Providers, and a "Write it myself" button.
- Provider down, key invalid, out of credits: the record gets
  `needs_attention`, the error is shown with the provider's fix text ("create
  a new key at …"), the fallback chain runs if the user built one, otherwise
  the step offers Retry, Change provider, or Manual.
- Revoye: no device online → "Start Revoye Desk on your machine" and the
  job stays queued; the UI shows the queue position and a Cancel that
  deletes the job. Agent busy → wait, with the fleet line updated once a
  minute. Job failed after N attempts → shown with N. Content pruned →
  "run it again".
- Response fails schema validation: one automatic retry with the validation
  error in the prompt; then manual with the raw text visible.
- Diff too large: per-file summaries first, then a combined draft; the UI
  says the draft was made from summaries.
- Draft suggests a version lower than or equal to the previous: the app
  overrides to the next patch and says so.
- A model refuses (`Safety`): shown as such, no fallback, manual offered.
- The user cancels the Draft step: the poll loop stops and, for Revoye, the
  job is cancelled server-side.

### 12.5 User actions

- Cancel during Write: rollback from snapshot (12.6).
- Cancel during Build or Preview: staged files removed, working copy
  reverted; nothing else changed.
- Cancel during Publish after the trunk commit: cannot be undone; the UI
  says so before Publish and offers Resume afterwards.
- Closing the app mid-run: the journal records the last completed step; on
  next open the project shows "Interrupted at step N" with Resume or Discard.
- Two app instances: the per-project lock makes the second one read-only
  for that project.

### 12.6 Rollback

Before Step 3 writes anything, each file to be modified is copied to
`<app-data>/svnpush/runs/<slug>/<timestamp>/snapshot/`. Cancel or failure
before Publish restores those bytes exactly. After a successful publish the
snapshot is kept with the journal for seven days, then pruned.

---

## 13. Data model (core types)

```
PluginFacts     { name, slug, main_file, header: Header, readme: Readme,
                  versions: Vec<VersionSource>, git: Option<GitFacts>,
                  previous: Option<Version> }
VersionSource   { label, path, value, kind: Header|StableTag|Changelog|Custom }
Readme          { headers: Map, short_description, sections: Vec<Section>,
                  changelog: Vec<ChangelogEntry>, upgrade_notice: Vec<…> }
ReleaseDraft    { version, reason, changelog_markdown, upgrade_notice,
                  summary, provider: { id, kind, label }, fell_back_from: Option<id> }
CheckResult     { id, severity: Block|Warn, status: Pass|Fail|Skip,
                  message, fix: Option<String>, paths: Vec<PathBuf> }
Package         { root, files: Vec<PackagedFile { rel, size, hash }>,
                  zip_path, sha256 }
SvnPreview      { trunk: Delta, assets: Delta, messages, tag_url }
Delta           { added, modified, deleted: Vec<PathBuf> }
ProviderRecord  { id, kind, label, model, base_url, has_key, is_default,
                  needs_attention, created_at, requests_this_month }
RunJournal      { id, slug, version, started, steps: Vec<StepRecord>,
                  revisions: { trunk: Option<u64>, tag: Option<u64> },
                  outcome: Complete|DryRun|Failed(step)|Cancelled|Interrupted }
```

The run is a state machine:

```
Idle → Detecting → Drafting → AwaitingApproval → Writing → Verifying
     → Building → Previewing → AwaitingPublish → Publishing → Verified
Any step may go to Failed(step) or Cancelled. DryRun ends at Previewing.
Interrupted is derived from a journal with no terminal outcome.
```

---

## 14. Coding standards and quality gates

### 14.1 Rust

- `rustfmt` default, `clippy` with `-D warnings` and the `pedantic` group
  enabled minus a short documented allow-list.
- No `unwrap` or `expect` outside tests. Errors via `thiserror` enums with
  stable string codes used by the UI.
- Public functions documented. Modules under 500 lines; split by
  responsibility, not by size.
- Async only where I/O happens. No global mutable state; the shell owns the
  app state and passes handles.
- `tracing` for logs, one span per release step, secrets redacted by a
  layer, log files rotated in the app data directory.
- Dependencies reviewed for maintenance and licence; `cargo deny` in CI.

### 14.2 TypeScript and React

- `strict: true`, `noUncheckedIndexedAccess`, ESLint with the recommended
  and React hooks rules, Prettier.
- Components are function components, one per file, props typed, no default
  exports. No business logic in components: they call commands and render
  state.
- DTO types generated from Rust with `ts-rs` so the two sides cannot drift.
- CSS: one tokens file, one file per screen, BEM-style class names. No inline
  styles except for computed widths.

### 14.3 Repository hygiene

- Conventional Commits. Small pull requests. Every PR runs the full CI.
- `CHANGELOG.md` for the app, kept by hand, one line per user-visible change.
- `CONTRIBUTING.md` with setup, test and release steps in under a page.
- `docs/DECISIONS.md`: every decision the builder makes that the plan left
  open, one line each, dated.
- No generated files committed except lockfiles.

### 14.4 CI (GitHub Actions for the SVNpush repository)

- `check`: fmt, clippy, cargo deny, eslint, prettier, tsc.
- `test`: `cargo test` on ubuntu, windows, macos; frontend unit tests.
- `svn-integration`: creates a local repository with `svnadmin create`,
  seeds `trunk/tags/assets/branches`, and runs the full release protocol
  against `file://` URLs with a fixture plugin. This proves the sync, commit
  and tag logic without touching WordPress.org.
- `build`: `tauri-action` produces installers for all three platforms on
  tags; artefacts attached to the GitHub release; updater manifest
  published.

---

## 15. Testing strategy

| Layer | What | How |
|---|---|---|
| Unit (core) | Header parser, readme parser and writer, version compare and write, exclusion matching, zip layout, check functions, prompt builders, schema validation | `cargo test`, fixtures under `fixtures/plugins/` (minimal, composer-based, block-with-build, no-readme, two-main-files, stable-tag-trunk, pre-release, and a real-world header and readme pair modelled on a shipping plugin) |
| Unit (AI) | Every adapter's `build_request`, `parse_response`, `parse_error`, `list_models`, `poll`; the registry order; the router's resolution and fallback rules | Recorded JSON fixtures per provider under `fixtures/ai/`; Revoye fixtures cover queued, dispatched, succeeded, failed, expired, pruned and every error code in `REVOYE-API.md` |
| Property | Version write-then-read round trips; readme changelog insert keeps every other byte | `proptest` |
| Integration (SVN) | Full protocol against a local `file://` SVN repository, including first release, second release, tag-exists failure, trunk-committed-then-resume | `cargo test --features svn-integration` |
| Integration (AI) | The client's submit-poll-cancel loop against a mock server that plays the Revoye job lifecycle; rate limit with `Retry-After`; schema-invalid answer then retry | `wiremock` |
| Shell | Each Tauri command returns the DTO shape the UI expects | Rust tests with a mock reporter |
| UI | Store transitions for every run state; check table rendering; Providers form visibility rules per adapter meta; theme toggle persistence | `vitest` + Testing Library |
| End-to-end | Add project → dry run → report, on a fixture plugin | Playwright against the built app, smoke only, run on tags |
| Manual before each app release | One real dry run and one real publish of a throwaway plugin on WordPress.org owned by the maintainer; one real Revoye draft | Checklist in `docs/RELEASE.md` |

---

## 16. Repository layout

```
svnpush/
├── apps/
│   └── desktop/
│       ├── src/                    React UI
│       │   ├── screens/            Projects, Release, Providers, Vault, Settings
│       │   ├── components/         Checklist, DiffView, FileList, LogDrawer, ThemeToggle, ProviderForm
│       │   ├── store/              run store, project store, provider store
│       │   ├── styles/             tokens.css, base.css, one file per screen
│       │   ├── ipc/                typed wrappers around invoke and events
│       │   └── strings.ts          every user-visible string
│       └── src-tauri/
│           ├── src/                commands.rs, events.rs, state.rs, main.rs
│           ├── capabilities/       least-privilege permission sets
│           └── tauri.conf.json
├── crates/
│   ├── core/                       svnpush-core
│   │   └── src/
│   │       ├── detect/ version/ readme/ package/ verify/ svn/ vault/ run/ tools/
│   │       └── ai/
│   │           ├── provider.rs     contract, ProviderError, error_from_status, trim_base
│   │           ├── registry.rs     get, list (Revoye first)
│   │           ├── order.rs        FIRST_PARTY_KIND, revoye_first
│   │           ├── router.rs       resolve, generate, opt-in fallback
│   │           ├── client.rs       the only HTTP caller; poll loop; cancel
│   │           ├── prompts.rs      draft_release, summarise_file, explain_failures
│   │           ├── schemas.rs      JSON schemas + validation
│   │           └── adapters/
│   │               ├── revoye.rs   first-party, recommended
│   │               ├── gemini.rs
│   │               ├── claude.rs
│   │               ├── openai.rs   OpenAiCompatible factory + openai, deepseek, qwen, perplexity, openai_compatible, local
│   │               └── openrouter.rs
│   └── cli/                        phase 2, thin wrapper over core
├── fixtures/
│   ├── plugins/                    test plugins
│   └── ai/                         recorded provider responses
├── docs/
│   ├── PLAN.md                     this document
│   ├── DEVELOPMENT-PROMPT.md       the build instruction
│   ├── REVOYE-API.md               Revoye public API reference
│   ├── DECISIONS.md                decisions made during the build
│   ├── PROTOCOL.md                 section 5, kept in sync, user-facing
│   ├── ARCHITECTURE.md             section 4 with diagrams
│   └── RELEASE.md                  how to release SVNpush itself
├── .github/workflows/              check, test, svn-integration, build
├── Cargo.toml                      workspace
├── package.json                    workspace scripts (lint, test, dev, build)
├── CHANGELOG.md
├── CONTRIBUTING.md
├── LICENSE                         MIT
└── README.md
```

---

## 17. Milestones and definition of done

Each milestone ends with green CI and a short demo recording.

| # | Milestone | Deliverable | Done when |
|---|---|---|---|
| M0 | Scaffold | Workspace, Tauri app opens a blank window with the theme toggle, CI runs fmt/clippy/eslint/tsc | Installers build on all three platforms |
| M1 | Core: detect and versions | `detect`, `version`, `readme` modules with fixtures and tests | Every fixture parses; write-then-read round trips pass |
| M2 | Core: package and verify | `package`, `verify`, all V and W checks | Fixture plugins produce spec-correct zips; every check has a failing and a passing test |
| M3 | Core: SVN | `tools`, `svn` modules; sparse checkout, sync, commit, tag, verify | Integration suite passes against a local `file://` repository |
| M4 | Desktop: release flow without AI | Projects and Release screens, run store, journal, rollback, dry run, cancel, resume; manual draft form | A real dry run and a real publish of a throwaway plugin succeed from the UI |
| M5 | Vault, Providers, Settings | Keychain, Doctor, SVN accounts, the Providers screen with every adapter's form rules, fallback list | Credentials and keys round-trip on all three platforms; the Providers form shows and hides fields exactly per adapter meta |
| M6 | AI engine | Provider contract, registry, order, router, client with poll loop, all ten adapters, prompts, schemas, privacy notice, AI-off mode | Adapter fixtures and the mock-server lifecycle tests pass; a real Revoye draft and a real Gemini draft succeed; every failure path falls back to manual |
| M7 | Polish and 1.0 | Assets-only release, warnings W01–W10, keyboard and accessibility pass, diagnostics export, docs, signed installers, updater | Two external developers release a plugin with no help beyond the README |

Definition of done for 1.0: a developer with `svn` installed can add a plugin,
click Release, approve a draft from the provider of their choice, and have a
verified tag on WordPress.org in under two minutes after approving, with
every check in section 5.4 enforced and every edge case in section 12
handled or clearly reported.

---

## 18. Distributing SVNpush itself

- Windows: NSIS installer and MSI via `tauri-action`; a code-signing
  certificate is required to avoid SmartScreen warnings (budget item).
- macOS: DMG, signed and notarised with an Apple Developer ID (budget item).
- Linux: AppImage and `.deb`.
- Auto-update through the Tauri updater with a signed manifest hosted on the
  GitHub release.
- The app's version follows semver; its own changelog is hand-written, which
  is a good reminder of the problem it solves.

---

## 19. Decisions the owner still has to make

Decided: the license is MIT; Revoye is the recommended, first-listed
provider; the in-repo config file is `.svnpush.json`; the CLI ships after
1.0.

Still open, with the recommendation the build will follow unless told
otherwise:

1. **Code-signing budget**: a Windows certificate and an Apple Developer ID
   are paid. Without them the app still works but installers warn.
   Recommendation: ship 1.0 unsigned, sign at 1.1.
2. **Default model ids** for Gemini, Claude and OpenAI at launch. The build
   uses the current Flash-class, Sonnet-class and GPT ids from each vendor's
   documentation on the day the adapter is written and records them in
   `DECISIONS.md`. They are settings, not code.
3. **Revoye attribution**: the `metadata.source` value is `svnpush`. Change
   it if Revoye's dashboard wants something else.

---

## 20. Glossary

- **Package root** — the folder whose contents become `trunk/`.
- **Staged tree** — the copy of the package root after exclusions, from
  which both the zip and the trunk sync are made.
- **Sparse working copy** — an SVN checkout with only `trunk/` and `assets/`
  fully populated.
- **Run** — one execution of the Release Protocol, dry or real, with a
  journal.
- **Blocking check** — a verification that stops the release; cannot be
  disabled.
- **Warning** — a verification that is shown but does not stop the release.
- **Adapter** — one AI provider's request builder and response parser.
- **Provider record** — a configured instance of an adapter with a label,
  model and key.
- **Router** — the rule that picks which provider record runs.
- **Vault** — the OS keychain entries SVNpush owns.
