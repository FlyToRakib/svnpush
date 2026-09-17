import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useProjectStore } from "../store/projectStore";
import { useRunStore } from "../store/runStore";
import { DRAFT_CONTEXT, PREVIEW, PROJECT_PATH, runState, summary } from "../test/fixtures";
import { tauriMock } from "../test/tauriMock";
import { ReleaseScreen } from "./ReleaseScreen";

function open(state = runState("AwaitingApproval", "Draft", { draft_context: DRAFT_CONTEXT })) {
  useProjectStore.setState({ projects: [summary()], selectedPath: PROJECT_PATH, loaded: true });
  useRunStore.setState({ runs: { [PROJECT_PATH]: { state, logs: [], actionError: null } } });
  tauriMock.handle("current_run", () => state);
  tauriMock.handle("project_history", () => []);
  tauriMock.handle("list_projects", () => [summary()]);
  return render(<ReleaseScreen />);
}

describe("ReleaseScreen", () => {
  beforeEach(() => {
    useRunStore.setState({ runs: {}, listening: false });
  });

  it("shows what to do when no project is open", () => {
    useProjectStore.setState({ projects: [], selectedPath: null });
    render(<ReleaseScreen />);
    expect(screen.getByText("No project open")).toBeTruthy();
  });

  it("approves the edited draft from the Draft step", async () => {
    const user = userEvent.setup();
    open();
    const version = await screen.findByLabelText("Version");
    await user.clear(version);
    await user.type(version, "1.1.0");
    await user.click(screen.getByRole("button", { name: "Approve" }));
    const call = tauriMock.calls.find((c) => c.command === "approve_draft");
    expect(call?.args?.path).toBe(PROJECT_PATH);
    expect((call?.args?.draft as { version: string }).version).toBe("1.1.0");
  });

  it("asks for an explicit confirmation before publishing", async () => {
    const user = userEvent.setup();
    open(
      runState("AwaitingPublish", "Publish", { preview: PREVIEW, draft: DRAFT_CONTEXT.prefill }),
    );
    await user.click(await screen.findByRole("button", { name: "Publish" }));
    const dialog = screen.getByRole("dialog", { name: "Publish this release" });
    expect(within(dialog).getByText("someone")).toBeTruthy();
    expect(tauriMock.calls.some((c) => c.command === "confirm_publish")).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "Publish" }));
    const call = tauriMock.calls.find((c) => c.command === "confirm_publish");
    expect(call?.args).toEqual({
      path: PROJECT_PATH,
      trunkMessage: "Release 1.0.1",
      tagMessage: "Tag 1.0.1",
    });
  });

  it("renders a failed check with its fix and offers Cancel only while running", async () => {
    open(
      runState("Failed", "Verify", {
        error: {
          code: "CHECKS_FAILED",
          message: "Blocking checks failed.",
          fix: "Fix each failed check.",
        },
        checks: [
          {
            id: "V06",
            severity: "Block",
            status: "Fail",
            title: "Stable tag is this version",
            message: "Stable tag is trunk.",
            fix: 'Set "Stable tag: 1.0.1" in readme.txt.',
            paths: ["readme.txt"],
          },
        ],
      }),
    );
    expect(await screen.findByText("Blocking checks failed.")).toBeTruthy();
    expect(screen.getByText('Set "Stable tag: 1.0.1" in readme.txt.')).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    expect(screen.getByRole("button", { name: "Release" })).toBeTruthy();
  });

  it("opens the plugin page once after a verified publish", async () => {
    const published = runState("Verified", null, {
      publish: {
        trunk_revision: 12,
        tag_revision: 13,
        verification: { state: "Verified" },
        plugin_url: "https://wordpress.org/plugins/demo/",
        open_plugin_page: true,
        zip_path: null,
      },
    });
    const view = open(published);
    await screen.findByText("r13");
    act(() => {
      useRunStore.getState().receiveState(PROJECT_PATH, { ...published, notices: ["again"] });
    });
    view.rerender(<ReleaseScreen />);
    expect(tauriMock.opened).toEqual(["https://wordpress.org/plugins/demo/"]);
  });

  it("starts a dry run when the toggle is on", async () => {
    const user = userEvent.setup();
    open(runState("DryRunComplete", null));
    tauriMock.handle("start_run", () => runState("Idle", "Detect", { dry_run: true }));
    await user.click(screen.getByRole("checkbox", { name: "Dry run" }));
    await user.click(screen.getByRole("button", { name: "Dry run" }));
    expect(tauriMock.calls.find((c) => c.command === "start_run")?.args).toEqual({
      path: PROJECT_PATH,
      dryRun: true,
    });
  });
});
