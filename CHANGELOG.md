# Changelog

All notable changes to SVNpush are listed here, newest first.

## Unreleased

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
