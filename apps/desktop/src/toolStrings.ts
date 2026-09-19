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
  assets: {
    title: "Plugin images (icon, banner, screenshots)",
    intro:
      "WordPress.org shows these on your plugin page. Keep them in this folder: every release uploads them to SVN assets/, and Update assets uploads only them. They are never part of the plugin zip.",
    none: "There is no assets folder yet.",
    create: "Create the folder",
    open: "Open folder",
    checkAgain: "Check again",
    empty: "The folder is empty. Add your icon, banner and screenshots.",
    ok: "OK",
    columns: { file: "File", role: "What it is", pixels: "Size", status: "Status" },
    role: {
      Icon: "Icon",
      IconSvg: "Icon (SVG)",
      Banner: "Banner",
      Screenshot: "Screenshot",
      Blueprints: "Live preview",
      Unknown: "Not used",
    },
    guideTitle: "Names and sizes WordPress.org expects",
    guide: [
      { name: "icon-128x128.png", size: "128 × 128", note: "Icon. PNG, JPG or GIF, up to 1 MB." },
      { name: "icon-256x256.png", size: "256 × 256", note: "Icon for high-resolution screens." },
      { name: "icon.svg", size: "any", note: "Optional. Needs the PNG icons as a fallback." },
      { name: "banner-772x250.png", size: "772 × 250", note: "Banner. PNG or JPG, up to 4 MB." },
      {
        name: "banner-1544x500.png",
        size: "1544 × 500",
        note: "Banner for high-resolution screens.",
      },
      {
        name: "banner-772x250-rtl.png",
        size: "772 × 250",
        note: "Optional. For right-to-left languages.",
      },
      {
        name: "screenshot-1.png",
        size: "any",
        note: "PNG or JPG, up to 10 MB. One per line in the readme's == Screenshots ==: 1. matches screenshot-1.",
      },
    ],
    guideNote:
      "Names are lowercase. The folder is .wordpress-org, the name WordPress developers and tools such as the 10up deploy action use; you can choose another in Project settings → Assets folder.",
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
