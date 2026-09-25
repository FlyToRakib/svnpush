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
      "Builds a zip of the version that is in your plugin files right now, using your .distignore and pre-build command. Nothing is committed and your plugin folder is not changed. Use the zip to test the plugin on a WordPress site.",
    current: (version: string) =>
      `This builds version ${version}, the version in your plugin files.`,
    newVersionNote:
      "The new version number is written into your files at release step 3 (Write). A dry run puts your files back afterwards, so its package is only a preview and this button still builds the current version.",
    build: "Build package",
    rebuild: "Build again",
    building: "Building…",
    blocked: "A package check failed, so a release would stop at Build. Fix it before you release.",
    ready: (files: number) => `Built ${String(files)} file(s). Nothing was published.`,
    builtVersion: (version: string) => `Version ${version}`,
  },
  flow: {
    title: "How to release",
    steps: [
      "Check readme.txt and your plugin images below. Fix any errors.",
      "Optional: build the package and test the zip on a WordPress site.",
      "Tick Dry run and click Dry run. It goes through the release steps without publishing, then puts your files back.",
      "Untick Dry run and click Release. Approve the draft, check the files and confirm the publish.",
    ],
    assetsNote:
      "Changed only the icon, banner or screenshots? Use Update assets. It publishes the images without a new version.",
    hide: "Hide",
    show: "Show how to release",
  },
  groups: {
    before: "1. Before you release",
    beforeHint:
      "Optional checks. Each one runs again inside a release, so nothing here is required.",
    steps: "2. Release steps",
    stepsEmpty:
      "Click Dry run or Release at the top. The seven release steps appear here and wait for you where your approval is needed.",
    project: "Project",
    show: "Show",
    hide: "Hide",
  },
  dryRunDone: (tried: string, current: string) =>
    `Dry run finished. It tried version ${tried}, and your files are back at ${current}. When you are ready, untick Dry run and click Release.`,
};
