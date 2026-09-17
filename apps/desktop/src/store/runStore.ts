import { create } from "zustand";
import type { Decision } from "../ipc/bindings/Decision";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { LogLine } from "../ipc/bindings/LogLine";
import type { ReleaseDraft } from "../ipc/bindings/ReleaseDraft";
import type { RunState } from "../ipc/bindings/RunState";
import { commands } from "../ipc/commands";
import { onRunLog, onRunState } from "../ipc/events";
import { toErrorView } from "../ipc/tauri";

/** Lines kept per project in the log drawer. */
export const LOG_LIMIT = 2000;

export interface ProjectRun {
  state: RunState | null;
  logs: LogLine[];
  /** A command the UI sent was refused. */
  actionError: ErrorView | null;
}

const EMPTY: ProjectRun = { state: null, logs: [], actionError: null };

interface RunStore {
  runs: Record<string, ProjectRun>;
  listening: boolean;
  listen: () => Promise<void>;
  receiveState: (projectPath: string, state: RunState) => void;
  receiveLog: (projectPath: string, line: LogLine) => void;
  load: (path: string) => Promise<void>;
  start: (path: string, dryRun: boolean, assetsOnly?: boolean) => Promise<void>;
  approve: (path: string, draft: ReleaseDraft) => Promise<boolean>;
  publish: (path: string, trunkMessage: string, tagMessage: string) => Promise<boolean>;
  decide: (path: string, decision: Decision) => Promise<boolean>;
  cancel: (path: string) => Promise<void>;
  resume: (path: string, runId: string) => Promise<void>;
  discard: (path: string, runId: string) => Promise<void>;
  clearError: (path: string) => void;
}

/** Whether a run is still going (not ended). */
export function isActive(state: RunState | null): boolean {
  if (!state) {
    return false;
  }
  return !["Verified", "PublishedUnverified", "DryRunComplete", "Failed", "Cancelled"].includes(
    state.phase,
  );
}

/** The run store: one RunState per project, rendered as-is. No workflow logic lives here. */
export const useRunStore = create<RunStore>((set, get) => {
  const patch = (path: string, change: Partial<ProjectRun>) => {
    set((s) => ({ runs: { ...s.runs, [path]: { ...(s.runs[path] ?? EMPTY), ...change } } }));
  };

  const attempt = async (path: string, action: () => Promise<unknown>): Promise<boolean> => {
    try {
      await action();
      patch(path, { actionError: null });
      return true;
    } catch (error) {
      patch(path, { actionError: toErrorView(error) });
      return false;
    }
  };

  return {
    runs: {},
    listening: false,

    listen: async () => {
      if (get().listening) {
        return;
      }
      set({ listening: true });
      await onRunState((event) => {
        get().receiveState(event.project_path, event.state);
      });
      await onRunLog((event) => {
        get().receiveLog(event.project_path, event.line);
      });
    },

    receiveState: (projectPath, state) => {
      patch(projectPath, { state });
    },

    receiveLog: (projectPath, line) => {
      const logs = [...(get().runs[projectPath]?.logs ?? []), line];
      patch(projectPath, { logs: logs.length > LOG_LIMIT ? logs.slice(-LOG_LIMIT) : logs });
    },

    load: async (path) => {
      await attempt(path, async () => {
        const state = await commands.currentRun(path);
        if (state) {
          patch(path, { state });
        }
      });
    },

    start: async (path, dryRun, assetsOnly = false) => {
      patch(path, { logs: [] });
      await attempt(path, async () => {
        patch(path, { state: await commands.startRun(path, dryRun, assetsOnly) });
      });
    },

    approve: (path, draft) => attempt(path, () => commands.approveDraft(path, draft)),

    publish: (path, trunkMessage, tagMessage) =>
      attempt(path, () => commands.confirmPublish(path, trunkMessage, tagMessage)),

    decide: (path, decision) => attempt(path, () => commands.aiDecision(path, decision)),

    cancel: async (path) => {
      await attempt(path, () => commands.cancelRun(path));
    },

    resume: async (path, runId) => {
      patch(path, { logs: [] });
      await attempt(path, async () => {
        patch(path, { state: await commands.resumeRun(path, runId) });
      });
    },

    discard: async (path, runId) => {
      await attempt(path, () => commands.discardRun(path, runId));
    },

    clearError: (path) => {
      patch(path, { actionError: null });
    },
  };
});
