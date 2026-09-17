import type { Phase } from "../ipc/bindings/Phase";
import type { ProjectSummary } from "../ipc/bindings/ProjectSummary";
import type { RunState } from "../ipc/bindings/RunState";
import type { Step } from "../ipc/bindings/Step";
import type { StepStatus } from "../ipc/bindings/StepStatus";

export const PROJECT_PATH = "C:/plugins/demo";

const STEPS: Step[] = ["Detect", "Draft", "Write", "Verify", "Build", "Preview", "Publish"];

export function summary(overrides: Partial<ProjectSummary> = {}): ProjectSummary {
  return {
    project: {
      path: PROJECT_PATH,
      name: "Demo Plugin",
      svn_url: "https://plugins.svn.wordpress.org/demo",
      slug: "demo",
      settings: {
        package_root: "",
        main_file: null,
        version_locations: [],
        required_paths: [],
        pre_build_command: null,
        assets_folder: null,
        allow_phar: false,
        post_publish_git_tag: false,
        post_publish_open_page: true,
        svn_account: null,
      },
      last_release: null,
      created: "2026-09-17T10:00:00Z",
    },
    version: "1.0.0",
    problem: null,
    unfinished: null,
    locked: false,
    account: "someone",
    ...overrides,
  };
}

/** A run state at `phase`, with steps before the active one done. */
export function runState(
  phase: Phase,
  active: Step | null,
  overrides: Partial<RunState> = {},
): RunState {
  const index = active ? STEPS.indexOf(active) : STEPS.length;
  const waiting = phase === "AwaitingApproval" || phase === "AwaitingPublish";
  return {
    id: "20260917-101530",
    project_path: PROJECT_PATH,
    dry_run: false,
    phase,
    steps: STEPS.map((step, i) => {
      let status: StepStatus = "Pending";
      if (i < index) status = "Done";
      if (i === index) status = waiting ? "Waiting" : phase === "Failed" ? "Failed" : "Running";
      return { step, status, summary: null };
    }),
    facts: null,
    git: null,
    server_tags: [],
    previous: "1.0.0",
    draft_context: null,
    draft: null,
    diffs: [],
    checks: [],
    package: null,
    preview: null,
    publish: null,
    error: null,
    notices: [],
    ...overrides,
  };
}

export const DRAFT_CONTEXT: NonNullable<RunState["draft_context"]> = {
  previous: "1.0.0",
  suggested_version: "1.0.1",
  changes: {
    source: "Git",
    base: "v1.0.0",
    files: [{ path: "demo.php", kind: "Modified" }],
    commits: ["Fix the widget"],
  },
  prefill: {
    version: "1.0.1",
    reason: "",
    changelog_markdown: "* Fix the widget",
    upgrade_notice: "",
    summary: "",
    provider: null,
    fell_back_from: null,
  },
};

export const PREVIEW: NonNullable<RunState["preview"]> = {
  trunk: { added: ["inc/new.php"], modified: ["demo.php", "readme.txt"], deleted: [] },
  assets: { added: [], modified: [], deleted: [] },
  trunk_message: "Release 1.0.1",
  tag_message: "Tag 1.0.1",
  tag_url: "https://plugins.svn.wordpress.org/demo/tags/1.0.1",
  svn_url: "https://plugins.svn.wordpress.org/demo",
  account: "someone",
  diffs: [{ path: "trunk/demo.php", diff: "--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n" }],
};
