# Decisions

Decisions made during the build where `docs/PLAN.md` was silent. One line
each, dated.

- 2026-09-17 — Node 22.22 LTS is the local toolchain (Node 20 is not installed); CI pins Node 20 and `engines` requires `>=20`, so both are supported.
- 2026-09-17 — Rust edition 2024 and workspace `rust-version = 1.88` (the floor set by `keyring` 4 and `zip` 8).
- 2026-09-17 — TypeScript 6.0 rather than 7.0: `typescript-eslint` supports `<6.1`.
- 2026-09-17 — Clippy allow-list: `module_name_repetitions` (types such as `SvnError` inside `svn` read better than the alternative), `missing_errors_doc` (every error is a typed enum with a stable code documented on the enum), `must_use_candidate` (noise on plain getters). `unwrap_used`, `expect_used` and `panic` are warnings, promoted to errors by `-D warnings`, and allowed only in tests via `clippy.toml`.
- 2026-09-17 — The theme choice persists in the webview's `localStorage` so it applies before first paint with no flash; it is a UI preference, not app state.
- 2026-09-17 — Navigation lists all five screens; Release shows an empty state until a project is opened from Projects.
- 2026-09-17 — Generated TypeScript bindings live in `apps/desktop/src/ipc/bindings/` (git-ignored) via `TS_RS_EXPORT_DIR` in `.cargo/config.toml`; `npm run typecheck` and `npm test` regenerate them with `cargo test export_bindings`.
- 2026-09-17 — App icons are rendered once from `icons/source.svg` with `cargo tauri icon` and committed, because the bundler needs them at build time; only desktop sizes are kept.
- 2026-09-17 — `cargo deny` checks `unmaintained` for direct dependencies only; the flagged crates (`unic-*`, `proc-macro-error`) are deep in the Tauri and GTK tree.
- 2026-09-17 — App identifier `com.degird.svnpush`.
- 2026-09-17 — "Installers build on all three platforms" (M0) is verified locally for Windows (NSIS and MSI); macOS and Linux are built by the `build` workflow, which cannot run from this machine.
- 2026-09-17 — Crates added for M1: `thiserror` (typed errors with stable codes, plan §4), `serde`/`serde_json` (DTOs and config files), `regex` (header scanning and user-configured version locations), `semver` (version ordering with normalisation), `ts-rs` (DTO types for the UI, plan §14.2); dev-only `proptest` (round-trip properties, plan §15) and `tempfile` (filesystem tests).
- 2026-09-17 — Errors that reach the UI implement one `Coded` trait (`code()` plus optional `fix()`); the shell turns any of them into a single `{ code, message, fix }` DTO.
- 2026-09-17 — File changes are composed in an in-memory `EditSet` before anything is written, because two sources can edit the same file (a version constant inside the main plugin file) and the diff, snapshot and journal all need the before/after pair.
- 2026-09-17 — `Readme.headers` is an ordered list with line numbers rather than a map, so fixes can name the line; lookups are case-insensitive.
- 2026-09-17 — `PluginFacts.readme` is optional: a plugin without `readme.txt` still detects, and V05/V10 report the missing file.
- 2026-09-17 — The readme parser reads the WordPress.org `=`-heading syntax only (`=== Name ===`, `== Section ==`, `= Entry =`); Markdown-style `##` readmes are not parsed in 1.0.
- 2026-09-17 — Writing a changelog entry for a version that already has one replaces that entry's body and keeps its title (for example a date), instead of adding a duplicate. A missing `== Changelog ==` section is appended at the end; a missing `== Upgrade Notice ==` is created directly after the changelog when the draft has a notice.
- 2026-09-17 — Version sources with no value are reported as empty rather than skipped, so V01 names them; a missing readme contributes no readme sources. A custom location that matches more than once yields one source per match.
- 2026-09-17 — Slugs are lowercase letters, digits and hyphens (the WordPress.org rule); the slug is the SVN URL's last segment, ignoring a trailing `/trunk`.
- 2026-09-17 — Git facts and the previous released version are added to `PluginFacts` by the run (M4), because they need the `tools` and `svn` modules (M3); `detect` itself stays a pure local-disk read.
- 2026-09-17 — The real-world fixture copies AuthDock's `readme.txt` and lines 1–28 of `authdock.php` (the header block and its `ABSPATH` guard, so V09 has a real example).
- 2026-09-17 — `fixtures/**` is marked `-text` in `.gitattributes` so byte-exact inputs (line endings, BOM) are never normalised by git.
- 2026-09-17 — Crates added for M2: `ignore` (gitignore semantics for `.distignore`, plan §7.1), `walkdir` (follows symlinks and reports loops, already a dependency of `ignore`), `blake3` (content hashes, plan §7.4), `zip` with only the `deflate-flate2-zlib-rs` feature (the package zip), `sha2` (the `.zip.sha256` checksum).
- 2026-09-17 — rustfmt uses `use_small_heuristics = "Max"` so short expressions stay on one line.
- 2026-09-17 — Integration test files allow `unwrap`/`expect`/`panic` at file level; clippy's `allow-*-in-tests` covers only `#[test]` functions, not their helpers.
- 2026-09-17 — The hard-excluded list matches case-insensitively (`Thumbs.db`, `*.ZIP`); `.distignore` keeps git's case-sensitive semantics.
- 2026-09-17 — The zip holds explicit directory entries (`<slug>/`, `<slug>/includes/`) alongside files, all sorted, with the zip epoch timestamp and fixed permissions, so the same tree always produces the same SHA-256.
- 2026-09-17 — The `.zip.sha256` file uses the `sha256sum` format (`<hex>  <name>`) so `sha256sum -c` verifies it.
- 2026-09-17 — V12 treats `.zip .tar .gz .tgz .rar .7z` as archives and `.exe .dll .msi` as executables; `.phar` is blocked unless the project allows it.
- 2026-09-17 — V09 accepts a `defined( 'ABSPATH' )` or `defined( 'WPINC' )` check anywhere in the main file (whitespace-insensitive).
- 2026-09-17 — Checks that cannot run yet report Skip with the reason: V13 before Build creates the zip, V15 before Preview creates the working copy. Skip never blocks; Build and Preview re-run them.
- 2026-09-17 — W02 compares major.minor only, the precision WordPress.org uses for "Tested up to".
- 2026-09-17 — Source paths longer than 260 characters are listed on the package (`long_paths`) and shown in Build as a notice, not as an eleventh warning.
- 2026-09-17 — Builds are pruned to the three most recently modified version folders per plugin.
- 2026-09-17 — Crates added for M3: `tokio` (`process`, `io-util`, `time`, `sync`, `macros`, `rt`; async child processes with streamed output, plan §4), `tokio-util` (the `CancellationToken` the plan names), `roxmltree` (read-only parsing of `svn status --xml`, avoiding fragile column parsing).
- 2026-09-17 — The 60-second network timeout (plan §12.2) is Subversion's own `servers:global:http-timeout=60`, passed with `--config-option` on every server call: it bounds a silent connection without cutting off a long but progressing commit. Cancellation kills the child process.
- 2026-09-17 — `svn` runs with `LC_MESSAGES=C` and `LANGUAGE=C` (English messages) but without overriding `LC_ALL`/`LC_CTYPE`, so non-ASCII file names keep working; errors are classified by Subversion's locale-independent `E` codes.
- 2026-09-17 — Server calls always pass `--no-auth-cache`, and `--username`/`--password-from-stdin` when credentials are given; `svn add` passes `--no-auto-props --no-ignore` so a user's global auto-props never set `svn:eol-style` and global ignores never silently drop a packaged file.
- 2026-09-17 — Paths are passed to `svn add/delete/propset` in batches of 100 to stay far below command-line length limits, instead of a `--targets` temp file inside the working copy.
- 2026-09-17 — `tag` refuses when `tags/<version>` already exists (`SVN_TAG_EXISTS`): the integration suite showed `svn copy` into an existing folder nests the copy (`tags/1.0.0/trunk`) instead of failing. The copy is pinned to the recorded trunk revision (`trunk@REV`).
- 2026-09-17 — Tag verification retries three times ten seconds apart (thirty seconds) before reporting "published, unverified".
- 2026-09-17 — The dry-run revert also deletes unversioned files the sync left behind, because `svn revert` keeps added files on disk.
- 2026-09-17 — The `Secret` type has redacted `Debug`/`Display`, no `Clone` and no `Serialize`; it does not claim to zero memory on drop, which the compiler may optimise away.
- 2026-09-17 — The `svn-integration` CI job runs on Linux and macOS, where Subversion installs from the package manager in seconds; Windows runs the suite locally.
