/**
 * The Help screen's copy, reached as `S.help` from `strings.ts` (kept in its
 * own file so the string table stays readable). Short, factual, second person.
 */
export const HELP = {
  title: "Help",
  subtitle: "Get ready to release, and see how SVNpush works.",
  welcome:
    "Welcome to SVNpush. Check the items below once, and you can release a plugin with one click.",
  setupTitle: "Get ready",
  ready: "Ready",
  optional: "Optional",
  todo: "To do",
  svn: {
    title: "Subversion (svn)",
    why: "WordPress.org stores plugins in Subversion, so SVNpush needs the svn command to publish.",
    missing: "Subversion is not installed yet.",
    install: "Install Subversion",
    installing: "Installing… Windows or macOS may ask for your permission.",
    installed: "Subversion is installed.",
    failed: "The install did not finish.",
    runYourself: "Run this command in a terminal, then click Check again:",
    willRun: "SVNpush runs:",
    checkAgain: "Check again",
  },
  account: {
    title: "WordPress.org SVN account",
    missing:
      "Add your WordPress.org username and SVN password (Profile → Account & Security → Subversion password). It is not your GitHub password.",
    ready: (count: number) => `${String(count)} account(s) in Vault.`,
    open: "Open Vault",
  },
  ai: {
    title: "AI provider",
    missing: "Optional. Add one to have release notes drafted for you, or write them yourself.",
    ready: (count: number) => `${String(count)} provider(s) set up.`,
    open: "Open Providers",
  },
  project: {
    title: "Your plugin",
    missing: "Add your plugin folder and its WordPress.org SVN URL.",
    ready: (count: number) => `${String(count)} plugin(s) added.`,
    open: "Open Projects",
  },
  howTitle: "How a release works",
  how: [
    "Detect: SVNpush reads your plugin's version, readme and the releases already on WordPress.org.",
    "Changes and draft: it lists what changed, and the AI (or you) writes the new version and changelog. You approve it.",
    "Write: the version and changelog are written into your files, shown as a diff.",
    "Verify: 16 safety checks and 10 warnings run. A failed check stops the release.",
    "Build: you check which files will be released; the package is built.",
    "Preview SVN: every file to add, change or delete on WordPress.org is listed.",
    "Publish: you confirm, and SVNpush commits trunk, creates the tag and checks it is live.",
  ],
  tip: "Turn on Dry run to rehearse everything without publishing. Your files are restored afterwards.",
  filesTitle: "Which files are released",
  files:
    "Your git repository can keep docs, tests and notes. A .distignore file in your plugin folder decides what stays out of the release. If you have none, SVNpush suggests one at the Build step for you to check and save. Hidden files and folders are left out by default, and files that hold secrets are never released.",
  problemsTitle: "Common problems",
  problems: [
    {
      q: "Subversion was not found",
      a: "Use Install Subversion above, or set its path in Settings → Tools.",
    },
    {
      q: "The commit is refused (E170001)",
      a: "Use your WordPress.org SVN password, not your account or GitHub password, and check the username is exactly your WordPress.org username.",
    },
    {
      q: "Nothing to release",
      a: "Your files match the released version. Make your changes, then release again.",
    },
    {
      q: "A check failed",
      a: "Each failed check shows its fix. With an AI provider, SVNpush also explains it and can suggest readme fixes.",
    },
    {
      q: "The app closed in the middle of a release",
      a: "Open the project: Resume finishes the release, or Discard puts your files back.",
    },
  ],
};
