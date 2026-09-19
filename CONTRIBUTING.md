# Contributing

## Setup

1. Install Rust (stable, via `rustup`), Node 22.22 or newer, and the
   [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your
   platform.
2. Install Subversion 1.10+ (needed by the SVN integration tests).
3. `npm ci` at the repository root.

## Run

```bash
npm run dev
```

## Check and test

Every commit must pass the full set, exactly as CI runs it:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo test --workspace
npm run lint
npm run typecheck
npm test
```

The SVN integration suites need `svn` and `svnadmin` on `PATH`:

```bash
cargo test -p svnpush-core --features svn-integration --test svn_integration --test run_integration --test run_features
```

TypeScript DTO types are generated from the Rust types with `ts-rs`
(`npm run gen:bindings`); `typecheck` and `test` regenerate them first. They
are never committed.

## Commits

Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `chore:`), one
logical change per commit, and a body that explains why. Add one line to
`CHANGELOG.md` for every user-visible change. Record any decision the plan
leaves open as one dated line in `docs/DECISIONS.md`.

## Release

See `docs/RELEASE.md`.
