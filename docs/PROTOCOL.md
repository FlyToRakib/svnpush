# The Release Protocol

This document describes what SVNpush does when you click **Release**, as
implemented. The specification is [PLAN.md §5](PLAN.md). The engine lives in
`crates/core/src/run/`.

A release is seven steps. Each step is a card in the checklist. A step is
Pending, Running, Waiting for you, Done, Failed, or Skipped. The UI receives
the whole run state after every change and renders it. It holds no workflow
logic of its own.

```
Detect → Changes and draft → Write → Verify → Build → Preview SVN → Publish
                  ▲ Approve         ▲ Apply fixes   ▲ Check files      ▲ Publish
```

Four moments wait for you: approving the draft, applying suggested fixes
after a failed check, checking the files to release, and confirming
Publish. Everything before the
trunk commit can be cancelled and is rolled back.

## Before the run

- **Project lock.** A run takes an OS file lock in
  `<app-data>/svnpush/runs/<slug>/`. A second window or a second run for the
  same plugin is refused with `RUN_LOCKED`.
- **Journal.** A JSON journal, `runs/<slug>/<run id>.json`, is written at the
  start and after every step. It records the version, the diffs written, the
  server revisions, the outcome and the snapshot folder. If the app closes
  mid-run, the journal has no outcome and the project page offers Resume or
  Discard.

## 1. Detect

- Subversion must be available (`TOOLS_SVN_UNAVAILABLE` otherwise).
- Reads the plugin header of the main file (auto-detected or set in project
  settings), `readme.txt`, and every version source: the header `Version`,
  the readme `Stable tag`, and each extra version location.
- With git: branch, uncommitted files, the last tag and the commit subjects
  since it.
- Lists `tags/` on the server. If the server cannot be reached, the run
  continues with a notice and uses the local working copy only.
- The **previous release** is the newest of the trunk working copy's header
  version and the server tags.

## 2. Changes and draft

1. **Change set.** With git and a last tag: `git diff --name-status <tag>`
   against the working tree plus untracked files, so uncommitted edits count
   (they are what will ship). Otherwise the package root is compared, file by
   file, with the trunk working copy (checked out anonymously first). If
   nothing changed, the run stops with `NOTHING_TO_RELEASE`.
2. **Pre-fill.** The version is the header version when you have already
   bumped it past the previous release, otherwise the next patch. The
   changelog is an entry already written for that version, otherwise the
   commit subjects as a list.
3. **AI draft** (unless AI is off for the project):
   - The provider is resolved: your Change choice for this run, then the
     project's pinned provider, then the default record, then the only record.
     With none, the step shows the `NoProvider` message, a link to Providers
     and the manual form.
   - Before a provider receives project data for the first time, an inline
     notice lists what is sent. Declining writes the draft by hand.
   - The material is the diff stat and per-file diffs, readme and PHP first,
     then JS/TS/CSS, then the rest. Each file is cut at 20,000 characters.
     Files matching the project's exclude patterns are withheld, and so are
     files that look like secrets (`.env*`, `*.pem`, `*.key`,
     `wp-config.php`), whatever the patterns say. Above 60,000 characters,
     up to 25 files are summarised one by one first, and the draft notes it.
   - The answer must match the `draft_release` JSON schema. One retry carries
     the validation error; after that the raw text is shown and you write the
     draft by hand. A refusal or a cut-off answer is shown as such, with no
     retry.
   - A suggested version that is invalid or not newer than the previous
     release is replaced with the next patch, and the draft says so.
   - An `Auth`, `Payment`, `RateLimit`, `Server`, `Network` or
     `ModelNotFound` failure rolls over to the next provider only if you built
     a fallback order. `Auth` and `Payment` mark the provider "needs
     attention".
   - While the AI works you can stop it, change the provider, or approve the
     form as it stands. Stopping a Revoye request deletes the queued job on
     the server. Revoye's queue position and fleet line are shown while you
     wait.
4. **Approve.** The draft (version, changelog entry, upgrade notice) is
   validated and nothing is written until you approve.

## 3. Write

1. Every file that will change is copied into the run's snapshot folder
   first.
2. The version is written to every source. The changelog entry is inserted
   at the top of `== Changelog ==` (or replaces an entry for the same
   version), and the upgrade notice is written when there is one. Edits are
   byte-preserving: line endings and untouched text stay as they were.
3. The unified diff of each written file is shown and stored in the journal.

## 4. Verify (the gate)

Every check runs. A failed blocking check stops the release; warnings never
do. No check can be switched off from the UI.

| ID | Blocking check |
|---|---|
| V01 | Every version source holds the same value |
| V02 | Version is valid |
| V03 | Version is newer than the previous release and not yet tagged |
| V04 | Readme changelog names this version first |
| V05 | Readme has every required header |
| V06 | Stable tag is this version |
| V07 | Short description is 150 characters or fewer |
| V08 | Text Domain equals the slug |
| V09 | Main plugin file blocks direct access |
| V10 | Package contains every required path |
| V11 | Package contains no forbidden path, including files that hold secrets (`.env*`, `*.pem`, `*.key`, `wp-config.php`, SSH keys) |
| V12 | Package contains no archive, executable or version-control folder |
| V13 | Zip entries use / and sit under the slug folder |
| V14 | Subversion 1.10 or newer is available |
| V15 | SVN working copy is conflict-free and up to date |
| V16 | SVN credentials are in the vault |
| V17 | readme.txt passes the WordPress.org readme validator (no validator errors) |

| ID | Warning |
|---|---|
| W01 | Git working tree is clean |
| W02 | Tested up to is the current WordPress version |
| W03 | Readme has five tags or fewer |
| W04 | Readme screenshots match the assets folder |
| W05 | Package and files are a reasonable size |
| W06 | Version is not a pre-release |
| W07 | Readme requirements match the plugin header |
| W08 | No packaged file is ignored by .gitignore |
| W09 | readme.txt has consistent line endings and no BOM |
| W10 | vendor/ is not oversized |
| W11 | readme.txt has no WordPress.org validator warnings |

Notes:

- V10 to V12 are skipped here when a pre-build command creates the package
  root; Build runs them against the staged tree. V10 to V13 always run again
  in Build against the staged files and the zip.
- V16 is skipped in a dry run, which never commits.
- V17 and W11 run the WordPress.org readme validator's rules locally
  (`crates/core/src/readme/validate.rs`). Its errors block (a plugin name, a
  GPL-compatible licence); its warnings flag what WordPress.org would ignore
  or cut off (version fields, Stable tag, tags, a short description over 150
  characters, sections over 2,500 words or 5,000 for Changelog and FAQ).
  Checks that need WordPress.org's own data are on the official page.
- W02 asks `api.wordpress.org` for the current WordPress version, at most
  once a day, and only while the Settings toggle is on.

**Explaining failures.** When a blocking check fails and a provider
resolves, the AI explains the failures and may suggest readme edits. The run
waits in *Waiting for fixes*. A suggestion can be applied only when it
targets `readme.txt` and its original text appears exactly once. Each is
shown as a diff first. Applying writes the readme (already in the
snapshot), adds the diff to Step 3's list, and runs Verify again. Code is
never edited. **Stop the release** ends the run as `CHECKS_FAILED`, and
local changes are rolled back.

## 5. Build

1. The pre-build command runs in the project folder when one is set. It is
   shown in the log first, its output is streamed, and a non-zero exit stops
   the release (`HOOK_FAILED`).
2. **Check the files to release.** The run pauses here, showing what will be
   released and what is left out, when any of these is true:
   - the plugin has no `.distignore`: SVNpush proposes one to review, edit
     and save;
   - this is the first release;
   - top-level files or folders are not in trunk yet (marked **New**).

   Editing the rules updates both lists as you type. Saving writes
   `.distignore` to the plugin folder, so commit it. When a pre-build
   command creates the package root, that command decides the files and
   the rules are not editable here.
3. The package root is walked with the exclusion rules. `.distignore` is
   used if present. Otherwise the built-in rules apply: every hidden file and
   folder (`.*`), `node_modules`, `tests`, `/docs`, `/bin`, `/build`,
   `/dist`, and developer files such as `composer.json`, `package.json`,
   `phpunit.xml` and `README.md`. `/build` and `/dist` are kept when the
   plugin's PHP or `block.json` loads files from them. The hard-excluded
   list always applies: `.git`, `.svn`, `.hg`, `.svnpush.json`,
   `.distignore`, archives, `.DS_Store` and `Thumbs.db`. The tree is
   staged to `<app-data>/svnpush/builds/<slug>/<version>/<slug>/`, and a
   deterministic zip is built with its SHA-256 checksum. Only the last three
   versions are kept.
4. V10 to V13 run against the staged files and the zip.

## 6. Preview SVN

1. The sparse working copy `<app-data>/svnpush/wc/<slug>/` is checked out
   (trunk and assets fully, tags and branches empty) or updated. V15 must
   pass.
2. The staged tree is mirrored into `trunk/`, and the assets folder into
   `assets/`: new files are added, changed files overwritten, and removed
   files deleted, in batches of 100 paths.
3. Added, modified and deleted lists for trunk and assets are shown, with a
   diff per file (2 MB in total), the tag URL and the default commit
   messages.

A **dry run** stops here. It reverts the working copy and restores the
snapshot, so your plugin files are byte-for-byte unchanged. The report stays
on screen.

## 7. Publish

1. You confirm in a dialog that names the version, slug, SVN URL, account
   and change counts.
2. The SVN password is read from the OS keychain at that moment. It reaches
   `svn` over stdin (`--password-from-stdin`) with `--no-auth-cache`.
3. `svn commit` of trunk and assets. The revision is recorded in the journal
   immediately.
4. A server-side `svn copy` of trunk at that revision to `tags/<version>`.
   An existing tag is refused rather than nested.
5. Verification: `svn ls` of the tag and `svn cat` of its main file, retried
   three times over thirty seconds. If the tag cannot be verified, the run
   ends as *Published, not yet verified*.
6. Optional: a git commit of the release edits and a `v<version>` tag, and
   opening the plugin page.

## Cancel, rollback and resume

- **Before the trunk commit**, Cancel or a failure restores the snapshot and
  reverts the working copy. A cancelled Build, Preview or Publish also
  removes the staged build.
- **After the trunk commit**, nothing on the server is undone. The journal
  records the trunk revision, and the project page offers **Resume: create
  tag**, which retries only the tag copy and its verification.
- **After an interruption before the trunk commit**, Resume rolls the old
  run back and starts a fresh one of the same kind. Discard rolls it back
  and marks the journal discarded.

## Assets-only release

**Update assets** on the project page syncs and commits `assets/` alone,
with no version change and no tag (plan §8.7):

- Detect runs as usual. Changes and draft, Write and Build are skipped.
- Verify runs V14, V16 and W04. The project must have an assets folder
  (`ASSETS_FOLDER_MISSING` otherwise).
- Preview mirrors only `assets/`, checks V15, and stops with
  `NOTHING_TO_RELEASE` if the server already matches.
- Publish asks for one commit message and commits only `assets/`. The run
  ends as *Assets published*, and the Projects list keeps showing the last
  version release.

Dry run, Cancel, rollback and Resume behave as they do for a full release.
