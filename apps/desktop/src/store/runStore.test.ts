import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Phase } from "../ipc/bindings/Phase";
import { PROJECT_PATH, runState } from "../test/fixtures";
import { tauriMock } from "../test/tauriMock";
import { isActive, LOG_LIMIT, useRunStore } from "./runStore";

const PHASES: [Phase, boolean][] = [
  ["Idle", true],
  ["Detecting", true],
  ["Drafting", true],
  ["AwaitingApproval", true],
  ["Writing", true],
  ["Verifying", true],
  ["Building", true],
  ["Previewing", true],
  ["AwaitingPublish", true],
  ["Publishing", true],
  ["Verified", false],
  ["PublishedUnverified", false],
  ["DryRunComplete", false],
  ["Failed", false],
  ["Cancelled", false],
];

describe("runStore", () => {
  beforeEach(() => {
    useRunStore.setState({ runs: {}, listening: false });
  });

  it.each(PHASES)(
    "stores the %s state as received and knows whether it is active",
    (phase, active) => {
      const state = runState(phase, null);
      useRunStore.getState().receiveState(PROJECT_PATH, state);
      const stored = useRunStore.getState().runs[PROJECT_PATH]?.state;
      expect(stored).toEqual(state);
      expect(isActive(stored ?? null)).toBe(active);
    },
  );

  it("follows events for each project separately", async () => {
    await useRunStore.getState().listen();
    tauriMock.emit("run-state", { project_path: "a", state: runState("Detecting", "Detect") });
    tauriMock.emit("run-log", { project_path: "a", line: { stream: "Command", text: "svn ls" } });
    tauriMock.emit("run-state", { project_path: "b", state: runState("Failed", "Verify") });
    const runs = useRunStore.getState().runs;
    expect(runs.a?.state?.phase).toBe("Detecting");
    expect(runs.b?.state?.phase).toBe("Failed");
    // Log lines are added once per frame.
    await vi.waitFor(() => {
      expect(useRunStore.getState().runs.a?.logs).toHaveLength(1);
    });
    expect(useRunStore.getState().runs.b?.logs ?? []).toHaveLength(0);
  });

  it("keeps only the newest log lines, each with its own key", () => {
    const lines = Array.from({ length: LOG_LIMIT + 5 }, (_, i) => ({
      stream: "Stdout" as const,
      text: String(i),
    }));
    useRunStore.getState().receiveLogs(PROJECT_PATH, lines);
    const logs = useRunStore.getState().runs[PROJECT_PATH]?.logs ?? [];
    expect(logs).toHaveLength(LOG_LIMIT);
    expect(logs[0]?.text).toBe("5");
    expect(new Set(logs.map((l) => l.seq)).size).toBe(LOG_LIMIT);
  });

  it("keeps a newer state that arrived before the start command answered", async () => {
    await useRunStore.getState().listen();
    tauriMock.handle("start_run", () => {
      // The shell runs the release before it replies: svn is missing, so it fails at once.
      tauriMock.emit("run-state", {
        project_path: PROJECT_PATH,
        state: runState("Failed", "Detect"),
      });
      return runState("Idle", "Detect");
    });
    await useRunStore.getState().start(PROJECT_PATH, false);
    expect(useRunStore.getState().runs[PROJECT_PATH]?.state?.phase).toBe("Failed");
  });

  it("never lets a current_run reply replace a newer run from events", async () => {
    await useRunStore.getState().listen();
    tauriMock.handle("current_run", () => {
      // A release started while the reply was on its way.
      tauriMock.emit("run-state", {
        project_path: PROJECT_PATH,
        state: { ...runState("Detecting", "Detect"), id: "newer" },
      });
      return runState("Failed", "Verify");
    });
    await useRunStore.getState().load(PROJECT_PATH);
    expect(useRunStore.getState().runs[PROJECT_PATH]?.state?.id).toBe("newer");
  });

  it("flushes a full log queue at once instead of waiting for a frame", async () => {
    await useRunStore.getState().listen();
    for (let i = 0; i < LOG_LIMIT; i++) {
      tauriMock.emit("run-log", {
        project_path: PROJECT_PATH,
        line: { stream: "Stdout", text: String(i) },
      });
    }
    expect(useRunStore.getState().runs[PROJECT_PATH]?.logs).toHaveLength(LOG_LIMIT);
  });

  it("sends one start for a double click and keeps the old log until it is accepted", async () => {
    useRunStore.getState().receiveLogs(PROJECT_PATH, [{ stream: "Stdout", text: "old run" }]);
    let answer: (state: unknown) => void = () => undefined;
    tauriMock.handle("start_run", () => new Promise((resolve) => (answer = resolve)));
    const first = useRunStore.getState().start(PROJECT_PATH, false);
    const second = useRunStore.getState().start(PROJECT_PATH, false);
    expect(useRunStore.getState().runs[PROJECT_PATH]?.pending).toBe(true);
    expect(useRunStore.getState().runs[PROJECT_PATH]?.logs).toHaveLength(1);
    answer(runState("Idle", "Detect"));
    await Promise.all([first, second]);
    expect(tauriMock.calls.filter((c) => c.command === "start_run")).toHaveLength(1);
    const run = useRunStore.getState().runs[PROJECT_PATH];
    expect(run?.pending).toBe(false);
    expect(run?.logs).toHaveLength(0);
    expect(run?.actionError).toBeNull();
  });

  it("keeps the old log when the start is refused", async () => {
    useRunStore.getState().receiveLogs(PROJECT_PATH, [{ stream: "Stdout", text: "old run" }]);
    tauriMock.reject("start_run", {
      code: "RUN_ALREADY_ACTIVE",
      message: "A release is already running for this plugin.",
      fix: null,
    });
    await useRunStore.getState().start(PROJECT_PATH, false);
    const run = useRunStore.getState().runs[PROJECT_PATH];
    expect(run?.logs).toHaveLength(1);
    expect(run?.actionError?.code).toBe("RUN_ALREADY_ACTIVE");
  });

  it("starts a run with camelCase arguments and stores the first state", async () => {
    tauriMock.handle("start_run", () => runState("Idle", "Detect", { dry_run: true }));
    await useRunStore.getState().start(PROJECT_PATH, true);
    expect(tauriMock.calls.at(-1)).toEqual({
      command: "start_run",
      args: { path: PROJECT_PATH, dryRun: true, assetsOnly: false },
    });
    expect(useRunStore.getState().runs[PROJECT_PATH]?.state?.dry_run).toBe(true);
  });

  it("records a refused action as an error with its fix", async () => {
    tauriMock.reject("approve_draft", {
      code: "DRAFT_EMPTY_CHANGELOG",
      message: "The changelog entry is empty.",
      fix: "Describe what changed in this release.",
    });
    const ok = await useRunStore.getState().approve(PROJECT_PATH, {
      version: "1.0.1",
      reason: "",
      changelog_markdown: "",
      upgrade_notice: "",
      summary: "",
      provider: null,
      fell_back_from: null,
    });
    expect(ok).toBe(false);
    expect(useRunStore.getState().runs[PROJECT_PATH]?.actionError?.code).toBe(
      "DRAFT_EMPTY_CHANGELOG",
    );
  });
});
