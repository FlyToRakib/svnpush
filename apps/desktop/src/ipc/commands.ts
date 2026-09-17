import type { FolderInspection } from "./bindings/FolderInspection";
import type { ProjectSettings } from "./bindings/ProjectSettings";
import type { ProjectSummary } from "./bindings/ProjectSummary";
import type { ReleaseDraft } from "./bindings/ReleaseDraft";
import type { RunJournal } from "./bindings/RunJournal";
import type { RunState } from "./bindings/RunState";
import { call } from "./tauri";

/** Typed wrappers for every shell command. Argument names are camelCase on the wire. */
export const commands = {
  listProjects: () => call<ProjectSummary[]>("list_projects"),
  inspectFolder: (folder: string) => call<FolderInspection>("inspect_folder", { folder }),
  addProject: (folder: string, svnUrl: string, mainFile: string | null) =>
    call<ProjectSummary>("add_project", { folder, svnUrl, mainFile }),
  updateProject: (path: string, svnUrl: string, settings: ProjectSettings) =>
    call<ProjectSummary>("update_project", { path, svnUrl, settings }),
  removeProject: (path: string) => call<null>("remove_project", { path }),
  projectHistory: (path: string) => call<RunJournal[]>("project_history", { path }),
  startRun: (path: string, dryRun: boolean) => call<RunState>("start_run", { path, dryRun }),
  currentRun: (path: string) => call<RunState | null>("current_run", { path }),
  approveDraft: (path: string, draft: ReleaseDraft) => call<null>("approve_draft", { path, draft }),
  confirmPublish: (path: string, trunkMessage: string, tagMessage: string) =>
    call<null>("confirm_publish", { path, trunkMessage, tagMessage }),
  cancelRun: (path: string) => call<null>("cancel_run", { path }),
  resumeRun: (path: string, runId: string) => call<RunState>("resume_run", { path, runId }),
  discardRun: (path: string, runId: string) => call<null>("discard_run", { path, runId }),
  resetWorkingCopy: (path: string) => call<null>("reset_working_copy", { path }),
};
