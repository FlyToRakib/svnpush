# Releasing SVNpush

This document covers releasing SVNpush itself: installers for Windows,
macOS and Linux, built by GitHub Actions and delivered to existing installs
through the signed updater (plan §18).

## One-time setup: the updater signing key

The updater installs an update only if its signature matches the public key
compiled into the app (`plugins.updater.pubkey` in
`apps/desktop/src-tauri/tauri.conf.json`).

The public key in the repository was generated during development. Its
private key is not in the repository and is not available to you. **Before
the first public release, generate your own pair:**

1. Generate the key pair, and choose a password when asked:

   ```bash
   npm run tauri --workspace apps/desktop -- signer generate -w ~/.tauri/svnpush.key
   ```

   This writes `~/.tauri/svnpush.key` (private) and
   `~/.tauri/svnpush.key.pub` (public).
2. Paste the content of `svnpush.key.pub` into `plugins.updater.pubkey` in
   `tauri.conf.json` and commit it.
3. In the GitHub repository settings, under **Secrets and variables →
   Actions**, add:
   - `TAURI_SIGNING_PRIVATE_KEY`: the content of `~/.tauri/svnpush.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password
4. Keep a backup of the private key and its password outside the machine.

### Regenerating the key

Rotate the key only if the private key is lost or exposed. Installed copies
trust only the old public key, so they **cannot** update to a release
signed with the new one. After rotating:

1. Repeat the steps above with a new key and update both secrets.
2. Say in the release notes that existing users must download and install
   this version by hand once. Later updates work automatically again.

## Code signing

Plan §19.1 recommends shipping 1.0 unsigned and signing from 1.1. Without
signing, the installers work but the operating systems warn:

- **Windows**: SmartScreen shows "Windows protected your PC". Users choose
  *More info → Run anyway*. A code-signing certificate removes the warning.
- **macOS**: Gatekeeper blocks the first launch. Users Control-click the app
  and choose *Open*. Signing and notarising with an Apple Developer ID
  removes the warning. To sign, add the `APPLE_CERTIFICATE` (base64 `.p12`),
  `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
  `APPLE_PASSWORD` (an app-specific password) and `APPLE_TEAM_ID` secrets,
  and pass them as environment variables to the `tauri-action` step in
  `.github/workflows/build.yml`.
- **Linux**: AppImage and `.deb` need no signing.

Updater signatures (above) are separate from code signing and are always
required.

## Cutting a release

1. **Check that main is green.** Run the full check set locally (see
   `CONTRIBUTING.md`) and confirm CI passed on `main`.
2. **Choose the version.** Follow semver: patch for fixes, minor for new
   features, major for breaking changes.
3. **Bump the version** in all three places, which must match:
   - `Cargo.toml`: `[workspace.package] version`
   - `apps/desktop/src-tauri/tauri.conf.json`: `version`
   - `apps/desktop/package.json`: `version`

   Then run `cargo check` and `npm install` so `Cargo.lock` and
   `package-lock.json` pick up the change.
4. **Update `CHANGELOG.md`.** Rename `## Unreleased` to
   `## X.Y.Z — YYYY-MM-DD` and add a new empty `## Unreleased` above it.
5. **Commit and tag:**

   ```bash
   git commit -am "chore: release X.Y.Z"
   git tag vX.Y.Z
   git push origin main vX.Y.Z
   ```

6. **Let the build run.** The `build` workflow starts on the `v*` tag. For
   Windows (NSIS and MSI), macOS (a universal DMG and app bundle) and Linux
   (AppImage and `.deb`), `tauri-action` builds the installers, signs the
   updater artifacts, and uploads everything to a **draft** GitHub release
   named `SVNpush vX.Y.Z`. It also uploads `latest.json`, the updater
   manifest, which lists each platform's download URL and signature. On
   Windows the updater uses the NSIS installer.
7. **Check the draft.** Confirm there is an installer for every platform and
   that `latest.json` lists `windows-x86_64`, `darwin-aarch64`,
   `darwin-x86_64` and `linux-x86_64`, each with a non-empty `signature`.
   Install on at least one platform and open the app.
8. **Publish the draft.** Paste the version's `CHANGELOG.md` section into the
   release notes and publish. The updater reads
   `https://github.com/FlyToRakib/svnpush/releases/latest/download/latest.json`,
   and GitHub serves that URL only for a published, non-prerelease release.
   Until you publish, no installed copy sees the update.
9. **Check the update path.** Open an installed copy of the previous version
   and use **Settings → Check for updates**. It should offer the new version,
   install it and restart.

## If something goes wrong

- **The build failed on one platform.** Fix it on `main`, delete the draft
  release and the tag (`git push --delete origin vX.Y.Z`,
  `git tag -d vX.Y.Z`), then tag again.
- **An installer was published with a bad update.** Publish a higher patch
  version with the fix. The updater never downgrades, so do not reuse or
  delete a published version number.
- **Signature errors in `latest.json` or on update.** The secrets do not
  match `plugins.updater.pubkey`. Check both, then rebuild.
