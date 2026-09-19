import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AiTask } from "../ipc/bindings/AiTask";
import type { RunState } from "../ipc/bindings/RunState";
import { useProjectStore } from "../store/projectStore";
import { useRunStore } from "../store/runStore";
import { DRAFT_CONTEXT, PROJECT_PATH, runState, summary } from "../test/fixtures";
import { tauriMock } from "../test/tauriMock";
import { ReleaseScreen } from "./ReleaseScreen";

const REVOYE = { id: "prov_revoye", kind: "revoye", label: "Revoye" };
const LOCAL = { id: "prov_local", kind: "local", label: "Local model" };

function task(overrides: Partial<AiTask> = {}): AiTask {
  return {
    status: "Working",
    provider: REVOYE,
    choices: [REVOYE, LOCAL],
    summaries: null,
    job_status: null,
    queue_position: null,
    fleet: null,
    error: null,
    raw_text: null,
    privacy: null,
    ...overrides,
  };
}

function drafting(ai: Partial<AiTask>, extra: Partial<NonNullable<RunState["draft_ai"]>> = {}) {
  return runState("Drafting", "Draft", {
    draft_context: DRAFT_CONTEXT,
    draft_ai: {
      task: task(ai),
      draft: null,
      generation: 0,
      from_summaries: false,
      notice: null,
      ...extra,
    },
  });
}

function open(state: RunState, onOpenProviders = vi.fn()) {
  useProjectStore.setState({ projects: [summary()], selectedPath: PROJECT_PATH, loaded: true });
  useRunStore.setState({ runs: { [PROJECT_PATH]: { state, logs: [], actionError: null } } });
  tauriMock.handle("current_run", () => state);
  tauriMock.handle("project_history", () => []);
  tauriMock.handle("list_projects", () => [summary()]);
  tauriMock.handle("provider_adapters", () => []);
  tauriMock.handle("list_providers", () => ({ schema: 1, providers: [], fallback: [] }));
  render(<ReleaseScreen onOpenProviders={onOpenProviders} onOpenHelp={vi.fn()} />);
  return onOpenProviders;
}

const decisions = () =>
  tauriMock.calls.filter((c) => c.command === "ai_decision").map((c) => c.args?.decision);

describe("Release AI", () => {
  beforeEach(() => {
    useRunStore.setState({ runs: {}, listening: false });
  });

  it("shows the Revoye queue and fleet, and stops the AI on request", async () => {
    const user = userEvent.setup();
    open(
      drafting({
        job_status: "queued",
        queue_position: 3,
        fleet: {
          devices_total: 2,
          devices_online: 0,
          agents_total: 4,
          agents_idle: 0,
          queue_depth: 5,
          providers: [],
        },
      }),
    );
    expect(await screen.findByText("Waiting in the Revoye queue, position 3.")).toBeTruthy();
    expect(screen.getByText(/0 of 2 device\(s\) online/)).toBeTruthy();
    expect(screen.getByText(/Start Revoye Desk/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Stop and write it myself" }));
    expect(decisions()).toEqual([{ kind: "Manual" }]);
  });

  it("asks once before sending project data, with the Revoye note", async () => {
    const user = userEvent.setup();
    open(drafting({ status: "NeedsConsent", privacy: { provider: REVOYE, revoye: true } }));
    const dialog = await screen.findByRole("region", { name: "Before the AI sees your plugin" });
    expect(within(dialog).getByText(/send project data to Revoye/)).toBeTruthy();
    expect(within(dialog).getByText(/through Revoye Desk on your own machine/)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Send and continue" }));
    expect(decisions()).toEqual([{ kind: "AcceptPrivacy", provider_id: "prov_revoye" }]);
  });

  it("loads the AI draft into the form and notes an overridden version", async () => {
    const draft = {
      ...DRAFT_CONTEXT.prefill,
      version: "1.1.0",
      reason: "Adds an export.",
      changelog_markdown: "* Added export.",
      summary: "Adds export.",
      provider: LOCAL,
    };
    open(
      runState("AwaitingApproval", "Draft", {
        draft_context: DRAFT_CONTEXT,
        draft_ai: {
          task: task({ status: "Done", provider: LOCAL }),
          draft,
          generation: 1,
          from_summaries: true,
          notice: "The AI suggested 1.0.0. Using 1.1.0 instead.",
        },
      }),
    );
    expect((await screen.findByLabelText<HTMLInputElement>("Version")).value).toBe("1.1.0");
    expect(screen.getByText("Why this version: Adds an export.")).toBeTruthy();
    expect(screen.getByText(/Drafted by Local model/)).toBeTruthy();
    expect(screen.getByText(/written from per-file summaries/)).toBeTruthy();
    expect(screen.getByText("The AI suggested 1.0.0. Using 1.1.0 instead.")).toBeTruthy();
  });

  it("shows a failure with the raw answer and changes the provider for this run", async () => {
    const user = userEvent.setup();
    open(
      drafting({
        status: "Failed",
        error: {
          code: "AI_INVALID_ANSWER",
          message: "The AI's answer could not be used.",
          fix: null,
        },
        raw_text: "Sorry, no JSON.",
      }),
    );
    expect(await screen.findByText("Sorry, no JSON.")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Change" }));
    await user.selectOptions(screen.getByLabelText("AI provider for this run"), "prov_local");
    await user.click(screen.getByRole("button", { name: "Use" }));
    expect(decisions()).toEqual([{ kind: "Generate", provider_id: "prov_local" }]);
  });

  it("offers Providers and writing by hand when no provider is configured", async () => {
    const user = userEvent.setup();
    const openProviders = open(
      drafting({
        status: "NoProvider",
        provider: null,
        choices: [],
        error: { code: "AI_NO_PROVIDER", message: "No AI provider configured.", fix: null },
      }),
    );
    await user.click(await screen.findByRole("button", { name: "Open Providers" }));
    expect(openProviders).toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Approve" })).toBeTruthy();
  });

  it("applies only the chosen, applicable readme fixes", async () => {
    const user = userEvent.setup();
    open(
      runState("AwaitingFixes", "Verify", {
        explanation: {
          task: task({ status: "Done", provider: LOCAL }),
          text: "The short description is too long.",
          fixes: [
            {
              check_id: "V07",
              path: "readme.txt",
              original: "long",
              replacement: "short",
              diff: "--- a/readme.txt\n+++ b/readme.txt\n@@ -1 +1 @@\n-long\n+short\n",
              problem: null,
            },
            {
              check_id: "V09",
              path: "demo.php",
              original: "a",
              replacement: "b",
              diff: "",
              problem: "Only readme.txt edits can be applied. Make this change yourself.",
            },
          ],
        },
      }),
    );
    expect(await screen.findByText("The short description is too long.")).toBeTruthy();
    const apply = screen.getByRole("button", { name: "Apply selected fixes and check again" });
    expect((apply as HTMLButtonElement).disabled).toBe(true);
    const boxes = screen.getAllByRole("checkbox", { name: /Fixes V/ });
    expect((boxes[1] as HTMLInputElement).disabled).toBe(true);
    await user.click(boxes[0] as HTMLInputElement);
    await user.click(apply);
    await user.click(screen.getByRole("button", { name: "Stop the release" }));
    expect(decisions()).toEqual([{ kind: "ApplyFixes", fixes: [0] }, { kind: "Stop" }]);
  });
});
