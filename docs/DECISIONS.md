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
