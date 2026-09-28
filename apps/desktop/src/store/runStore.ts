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

/** A log line with its arrival number, which is its stable key in the drawer. */
export interface LogEntry extends LogLine {
  seq: number;
}

export interface ProjectRun {
  state: RunState | null;
  logs: LogEntry[];
  /** A command the UI sent was refused. */
  actionError: ErrorView | null;
  /** Start, Resume or Discard was sent and has not answered yet. */
  pending: boolean;
  /** Build package is running. It holds the project lock, so a release cannot start. */
  building: boolean;
}

const EMPTY: ProjectRun = {
  state: null,
  logs: [],
  actionError: null,
  pending: false,
  building: false,
};

interface RunStore {
  runs: Record<string, ProjectRun>;
  listening: boolean;
  listen: () => Promise<void>;
  receiveState: (projectPath: string, state: RunState) => void;
  receiveLogs: (projectPath: string, lines: LogLine[]) => void;
  load: (path: string) => Promise<void>;
  start: (path: string, dryRun: boolean, assetsOnly?: boolean) => Promise<void>;
  approve: (path: string, draft: ReleaseDraft) => Promise<boolean>;
  publish: (path: string, trunkMessage: string, tagMessage: string) => Promise<boolean>;
  decide: (path: string, decision: Decision) => Promise<boolean>;
  confirmFiles: (path: string, distignore: string | null) => Promise<boolean>;
  cancel: (path: string) => Promise<void>;
  resume: (path: string, runId: string) => Promise<void>;
  discard: (path: string, runId: string) => Promise<void>;
  setBuilding: (path: string, building: boolean) => void;
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

  // Log lines arrive one event each; they are added once per frame, so a
  // chatty svn command re-renders the drawer once a frame, not once a line.
  let nextSeq = 0;
  let queued: { path: string; line: LogLine }[] = [];
  let scheduled = false;
  const flushLogs = () => {
    scheduled = false;
    const batch = queued;
    queued = [];
    const byPath = new Map<string, LogLine[]>();
    for (const { path, line } of batch) {
      byPath.set(path, [...(byPath.get(path) ?? []), line]);
    }
    for (const [path, lines] of byPath) {
      get().receiveLogs(path, lines);
    }
  };
  const queueLog = (path: string, line: LogLine) => {
    queued.push({ path, line });
    if (!scheduled) {
      scheduled = true;
      if (typeof requestAnimationFrame === "function") {
        requestAnimationFrame(flushLogs);
      } else {
        setTimeout(flushLogs, 16);
      }
    }
  };

  // The shell starts a run before it answers, so run-state events can arrive
  // before the command's own reply. Those events are newer: keep them.
  const adopt = (path: string, before: RunState | null, returned: RunState) => {
    const now = get().runs[path]?.state ?? null;
    if (now !== before && now?.id === returned.id) {
      return;
    }
    patch(path, { state: returned });
  };

  // Start, Resume and Discard: one at a time per project. A new run's log
  // replaces the old one only once the shell has accepted it.
  const launch = async (
    path: string,
    action: () => Promise<RunState | null>,
    freshLog: boolean,
  ) => {
    if (get().runs[path]?.pending) {
      return;
    }
    flushLogs();
    const mark = nextSeq;
    const before = get().runs[path]?.state ?? null;
    patch(path, { pending: true });
    await attempt(path, async () => {
      const state = await action();
      if (freshLog) {
        flushLogs();
        patch(path, { logs: (get().runs[path]?.logs ?? []).filter((l) => l.seq >= mark) });
      }
      if (state) {
        adopt(path, before, state);
      }
    });
    patch(path, { pending: false });
  };

  return {
    runs: {},
    listening: false,

    listen: async () => {
      if (get().listening) {
        return;
      }
      set({ listening: true });
      const unlisten: (() => void)[] = [];
      try {
        unlisten.push(
          await onRunState((event) => {
            get().receiveState(event.project_path, event.state);
          }),
        );
        unlisten.push(
          await onRunLog((event) => {
            queueLog(event.project_path, event.line);
          }),
        );
      } catch (error) {
        // Without events the release page would never move; allow another try.
        for (const stop of unlisten) {
          stop();
        }
        set({ listening: false });
        console.error("Could not listen for run events", error);
      }
    },

    receiveState: (projectPath, state) => {
      patch(projectPath, { state });
    },

    receiveLogs: (projectPath, lines) => {
      const added = lines.map((line) => ({ ...line, seq: nextSeq++ }));
      const logs = [...(get().runs[projectPath]?.logs ?? []), ...added];
      patch(projectPath, { logs: logs.length > LOG_LIMIT ? logs.slice(-LOG_LIMIT) : logs });
    },

    load: async (path) => {
      const before = get().runs[path]?.state ?? null;
      await attempt(path, async () => {
        const state = await commands.currentRun(path);
        if (state) {
          adopt(path, before, state);
        }
      });
    },

    start: (path, dryRun, assetsOnly = false) =>
      launch(path, () => commands.startRun(path, dryRun, assetsOnly), true),

    approve: (path, draft) => attempt(path, () => commands.approveDraft(path, draft)),

    publish: (path, trunkMessage, tagMessage) =>
      attempt(path, () => commands.confirmPublish(path, trunkMessage, tagMessage)),

    decide: (path, decision) => attempt(path, () => commands.aiDecision(path, decision)),

    confirmFiles: (path, distignore) =>
      attempt(path, () => commands.confirmReleaseFiles(path, distignore)),

    cancel: async (path) => {
      await attempt(path, () => commands.cancelRun(path));
    },

    resume: (path, runId) => launch(path, () => commands.resumeRun(path, runId), true),

    discard: (path, runId) =>
      launch(
        path,
        async () => {
          await commands.discardRun(path, runId);
          return null;
        },
        false,
      ),

    setBuilding: (path, building) => {
      patch(path, { building });
    },

    clearError: (path) => {
      patch(path, { actionError: null });
    },
  };
});
