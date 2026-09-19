import type { AdapterInfo } from "./bindings/AdapterInfo";
import type { AppSettings } from "./bindings/AppSettings";
import type { Decision } from "./bindings/Decision";
import type { DoctorReport } from "./bindings/DoctorReport";
import type { FilePreview } from "./bindings/FilePreview";
import type { Fleet } from "./bindings/Fleet";
import type { FolderInspection } from "./bindings/FolderInspection";
import type { InstallOutcome } from "./bindings/InstallOutcome";
import type { InstallPlan } from "./bindings/InstallPlan";
import type { ModelList } from "./bindings/ModelList";
import type { ProviderInput } from "./bindings/ProviderInput";
import type { ProvidersFile } from "./bindings/ProvidersFile";
import type { ProviderTarget } from "./bindings/ProviderTarget";
import type { UpdateInfo } from "./bindings/UpdateInfo";
import type { VaultView } from "./bindings/VaultView";
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
  startRun: (path: string, dryRun: boolean, assetsOnly: boolean) =>
    call<RunState>("start_run", { path, dryRun, assetsOnly }),
  currentRun: (path: string) => call<RunState | null>("current_run", { path }),
  approveDraft: (path: string, draft: ReleaseDraft) => call<null>("approve_draft", { path, draft }),
  aiDecision: (path: string, decision: Decision) => call<null>("ai_decision", { path, decision }),
  previewReleaseFiles: (path: string, distignore: string) =>
    call<FilePreview>("preview_release_files", { path, distignore }),
  confirmReleaseFiles: (path: string, distignore: string | null) =>
    call<null>("confirm_release_files", { path, distignore }),
  confirmPublish: (path: string, trunkMessage: string, tagMessage: string) =>
    call<null>("confirm_publish", { path, trunkMessage, tagMessage }),
  cancelRun: (path: string) => call<null>("cancel_run", { path }),
  resumeRun: (path: string, runId: string) => call<RunState>("resume_run", { path, runId }),
  discardRun: (path: string, runId: string) => call<null>("discard_run", { path, runId }),
  resetWorkingCopy: (path: string) => call<null>("reset_working_copy", { path }),

  vaultView: () => call<VaultView>("vault_view"),
  saveAccount: (host: string, username: string, password: string) =>
    call<VaultView>("save_account", { host, username, password }),
  removeAccount: (host: string, username: string) =>
    call<VaultView>("remove_account", { host, username }),
  testAccount: (host: string, username: string) => call<string>("test_account", { host, username }),

  providerAdapters: () => call<AdapterInfo[]>("provider_adapters"),
  listProviders: () => call<ProvidersFile>("list_providers"),
  saveProvider: (input: ProviderInput) => call<ProvidersFile>("save_provider", { input }),
  removeProvider: (id: string) => call<ProvidersFile>("remove_provider", { id }),
  setDefaultProvider: (id: string) => call<ProvidersFile>("set_default_provider", { id }),
  clearProviderAttention: (id: string) => call<ProvidersFile>("clear_provider_attention", { id }),
  saveProviderFallback: (order: string[]) =>
    call<ProvidersFile>("save_provider_fallback", { order }),
  testProvider: (id: string) => call<string>("test_provider", { id }),
  listProviderModels: (target: ProviderTarget) =>
    call<ModelList>("list_provider_models", { target }),
  providerFleet: (target: ProviderTarget) => call<Fleet>("provider_fleet", { target }),

  getSettings: () => call<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => call<AppSettings>("save_settings", { settings }),
  runDoctor: () => call<DoctorReport>("run_doctor"),
  svnInstallPlan: () => call<InstallPlan>("svn_install_plan"),
  installSvn: () => call<InstallOutcome>("install_svn"),
  diagnostics: () => call<string>("diagnostics"),
  checkUpdate: () => call<UpdateInfo>("check_update"),
  installUpdate: () => call<null>("install_update"),
};
