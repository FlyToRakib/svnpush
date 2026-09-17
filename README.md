# SVNpush

A desktop app that takes a WordPress plugin from a local folder to a published
WordPress.org release in one click, safely: AI drafts the version and
changelog, deterministic checks decide whether the release may proceed, and
you click Publish.

## Requirements

- Subversion 1.10 or newer on `PATH`
  - Windows: TortoiseSVN with the command-line tools, `choco install svn`, or SlikSVN
  - macOS: `brew install subversion`
  - Linux: `apt install subversion` or your distribution's equivalent
- Git (optional): enables changelog drafts from your commit history

## Development

```bash
npm ci
npm run dev
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full check set and
[docs/PLAN.md](docs/PLAN.md) for the specification.

## License

MIT
