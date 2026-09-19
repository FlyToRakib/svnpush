# Changelog

All notable changes to SVNpush are listed here, newest first.

## Unreleased

- New Help tab: a setup checklist (Subversion, SVN account, AI provider, your plugin), a short guide to how a release works, which files are released, and common problems. It opens by itself the first time you start SVNpush.
- Install Subversion in one click from Help, through winget on Windows or Homebrew on macOS. The Linux packages install it automatically. If a release can't find Subversion, it offers Open Help.
- Before a release, SVNpush shows which files will go to WordPress.org and which are left out. If your plugin has no .distignore, it proposes one for you to edit and save. It also asks on the first release and when new files or folders appear.
- Hidden files and folders, docs/, bin/ and more developer files are now left out by default. A build/ or dist/ folder your plugin loads is kept.
- Files that hold secrets (.env, private keys, wp-config.php) can never be released.
- Plugins that live in a subfolder of a larger git repository now list their changes correctly.
- Update assets: sync and commit only your banners, icons and screenshots, with the same preview and confirmation and no version change.
- The AI data notice now appears inline in the step instead of a dialog, and takes keyboard focus.
- Input borders are easier to see, and screen readers announce each change of release phase.
- Side-by-side fields in Settings and Project settings line up.
- AI drafts in the release flow: the change set goes to your chosen provider, which suggests the version, changelog entry, upgrade notice and summary for you to edit and approve.
- Change the AI provider for one release from the Draft step; see Revoye's queue position and fleet while you wait, and stop the request at any time.
- A one-time notice before a provider first receives your project data.
- When a blocking check fails, the AI explains it and suggests readme fixes you can review as a diff and apply with one click before checking again.
- Project settings: choose the AI provider (default, a specific one, or off) and patterns for files the AI never sees. Files that look like secrets are always kept out.
- Every modal dialog now has its own title for screen readers.
- The theme toggles in the title bar and in Settings always agree.
- Providers screen: add Revoye (recommended), Gemini, Claude, OpenAI, OpenRouter, DeepSeek, Qwen, Perplexity, any OpenAI-compatible endpoint or a local model; test a provider, load your account's models, see Revoye's fleet, set the default, and opt into an ordered fallback.
- Vault screen: SVN accounts with passwords kept in your operating system keychain.
- Settings screen: theme, svn and git paths with Doctor, the WordPress version lookup toggle, Copy diagnostics, and update check.
- Warning W02 now compares Tested up to with the current WordPress version.
- Release flow: the seven-step checklist from detection to a verified tag, with a manual draft form.
- Dry run: preview the SVN changes without committing; your files are restored afterwards.
- Every blocking check and warning is shown with its fix; a failed blocking check stops the release.
- Cancel at any step; changes made before publishing are rolled back.
- Resume a release interrupted after the trunk commit by creating only the tag.
- Projects list with the current version and the last release.
- Project settings: package root, main file, extra version locations, required paths, pre-build command, assets folder and post-publish options.
- Application shell with Projects, Release, Providers, Vault and Settings navigation.
- System, Light and Dark theme toggle that remembers your choice.
