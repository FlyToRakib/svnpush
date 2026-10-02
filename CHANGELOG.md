# Changelog

All notable changes to SVNpush are listed here, newest first. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

## 0.1.0 — 2026-10-02

The first public release. SVNpush takes a WordPress plugin from a local
folder to a published, verified WordPress.org release.

### Releasing

- A seven-step release checklist: Detect, Changes and draft, Write, Verify,
  Build, Preview SVN, Publish. Each step shows exactly what it did.
- Every version source is updated together and shown as a diff: the plugin
  header, the readme Stable tag, the changelog, the upgrade notice, and any
  extra locations you configure (a constant, `package.json`).
- Seventeen blocking checks (V01–V17) and twelve warnings (W01–W12), each
  with its fix. A failed blocking check stops the release.
- Before the first release, or when new top-level files appear, SVNpush shows
  which files go to WordPress.org and which are left out, and proposes a
  `.distignore` when the plugin has none. Secret files (`.env`, private keys,
  `wp-config.php`) can never be released.
- Preview SVN lists every file to add, change or delete before you confirm
  Publish. Publishing commits trunk, creates the tag on the server and checks
  that the tag is live.
- Cancel at any step before the trunk commit, and your files are restored.
  A release interrupted after the trunk commit is finished with Resume.
- Dry run rehearses everything up to the SVN preview and restores your files.
- Update assets publishes only your icon, banners and screenshots, with no
  version change.
- Build package builds exactly what a release would publish, as a folder and
  a zip, without publishing.
- The readme check uses the WordPress.org readme validator's rules, offline,
  on every release and at any time from the project page.
- The plugin images card checks the names and pixel sizes WordPress.org
  expects.

### AI drafts (optional)

- Your chosen provider suggests the version, changelog entry, upgrade notice
  and summary from what changed. You edit and approve everything; a release
  never depends on AI.
- When a check fails, the AI can explain why and suggest readme fixes, which
  are applied only after you review the diff.
- Providers: Revoye, Gemini, Claude, OpenAI, OpenRouter, DeepSeek, Qwen,
  Perplexity, any OpenAI-compatible endpoint, and local models such as
  Ollama or LM Studio. Automatic fallback between providers is optional.
- A notice shows what will be sent before a provider first receives your
  project data. Only changes are sent, never the whole plugin, and files
  that look like secrets or match your exclude patterns are never sent.

### App

- The SVN password and API keys are stored in your operating system
  keychain, never in a file or a log.
- A setup checklist in Help, and one-click Subversion install through winget
  on Windows or Homebrew on macOS. The Linux packages install it
  automatically.
- Project settings, also shareable with a team as `.svnpush.json`: package
  root, main file, extra version locations, required paths, pre-build
  command, assets folder, SVN account, AI provider and exclude patterns.
- Light, dark and system themes; keyboard and screen reader support.
- Signed in-app updates from Settings → Check for updates.

### Known limitations

- The installers are not code-signed, so Windows SmartScreen and macOS
  Gatekeeper warn on first install. See the README for how to open the app.
- On macOS, quitting with Cmd+Q or from the Dock during a release does not
  ask first, unlike closing the window.
