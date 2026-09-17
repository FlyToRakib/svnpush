import { beforeEach, describe, expect, it } from "vitest";
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
    expect(runs.a?.logs).toHaveLength(1);
    expect(runs.b?.state?.phase).toBe("Failed");
    expect(runs.b?.logs).toHaveLength(0);
  });

  it("keeps only the newest log lines", () => {
    const store = useRunStore.getState();
    for (let i = 0; i < LOG_LIMIT + 5; i += 1) {
      store.receiveLog(PROJECT_PATH, { stream: "Stdout", text: String(i) });
    }
    const logs = useRunStore.getState().runs[PROJECT_PATH]?.logs ?? [];
    expect(logs).toHaveLength(LOG_LIMIT);
    expect(logs[0]?.text).toBe("5");
  });

  it("starts a run with camelCase arguments and stores the first state", async () => {
    tauriMock.handle("start_run", () => runState("Idle", "Detect", { dry_run: true }));
    await useRunStore.getState().start(PROJECT_PATH, true);
    expect(tauriMock.calls.at(-1)).toEqual({
      command: "start_run",
      args: { path: PROJECT_PATH, dryRun: true },
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
