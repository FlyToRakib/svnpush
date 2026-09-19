/**
 * Copy for the project page's tools, reached as `S.tools` from `strings.ts`
 * (kept in its own file so the string table stays under 500 lines).
 */
export const TOOLS = {
  readme: {
    title: "Readme check",
    intro:
      "Checks readme.txt with the same rules as the WordPress.org readme validator. Errors stop a release; warnings and notes do not.",
    check: "Check readme.txt",
    checking: "Checking…",
    official: "Open the official validator",
    officialHint:
      "The official validator also checks trademarked names, that contributors are real WordPress.org usernames, and how widely your tags are used.",
    clean: "No problems found. WordPress.org will read this readme as written.",
    level: { Error: "Error", Warning: "Warning", Note: "Note" },
    counts: (errors: number, warnings: number, notes: number) =>
      `${String(errors)} error(s), ${String(warnings)} warning(s), ${String(notes)} note(s)`,
  },
  build: {
    title: "Build package",
    intro:
      "Builds exactly what a release would publish, using your .distignore and pre-build command. Nothing is committed and your plugin folder is not changed. Use the zip to test the plugin on a WordPress site.",
    build: "Build package",
    building: "Building…",
    blocked: "A package check failed, so a release would stop at Build. Fix it before you release.",
    ready: (files: number) => `Built ${String(files)} file(s). Nothing was published.`,
  },
};
