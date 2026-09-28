import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useProjectStore } from "../store/projectStore";
import { useRunStore } from "../store/runStore";
import { DRAFT_CONTEXT, PREVIEW, PROJECT_PATH, runState, summary } from "../test/fixtures";
import { tauriMock } from "../test/tauriMock";
import { ReleaseScreen } from "./ReleaseScreen";

function open(
  state = runState("AwaitingApproval", "Draft", { draft_context: DRAFT_CONTEXT }),
  project = summary(),
) {
  useProjectStore.setState({ projects: [project], selectedPath: PROJECT_PATH, loaded: true });
  useRunStore.setState({
    runs: {
      [PROJECT_PATH]: { state, logs: [], actionError: null, pending: false, building: false },
    },
  });
  tauriMock.handle("current_run", () => state);
  tauriMock.handle("project_history", () => []);
  tauriMock.handle("list_projects", () => [project]);
  tauriMock.handle("provider_adapters", () => []);
  tauriMock.handle("list_providers", () => ({ schema: 1, providers: [], fallback: [] }));
  return render(<ReleaseScreen onOpenProviders={vi.fn()} onOpenHelp={vi.fn()} />);
}

describe("ReleaseScreen", () => {
  beforeEach(() => {
    useRunStore.setState({ runs: {}, listening: false });
  });

  it("shows what to do when no project is open", () => {
    useProjectStore.setState({ projects: [], selectedPath: null });
    render(<ReleaseScreen onOpenProviders={vi.fn()} onOpenHelp={vi.fn()} />);
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

  it("keeps the SVN preview open while it asks to confirm the publish", async () => {
    const user = userEvent.setup();
    open(
      runState("AwaitingPublish", "Publish", { preview: PREVIEW, draft: DRAFT_CONTEXT.prefill }),
    );
    const preview = await screen.findByRole("button", { name: /Preview SVN/ });
    expect(preview.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("inc/new.php")).toBeTruthy();
    // Your own toggle still wins.
    await user.click(preview);
    expect(preview.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText("inc/new.php")).toBeNull();
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
        assets_revision: null,
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
    view.rerender(<ReleaseScreen onOpenProviders={vi.fn()} onOpenHelp={vi.fn()} />);
    // Leaving the screen and coming back does not open it again.
    view.unmount();
    render(<ReleaseScreen onOpenProviders={vi.fn()} onOpenHelp={vi.fn()} />);
    await screen.findByText("r13");
    expect(tauriMock.opened).toEqual(["https://wordpress.org/plugins/demo/"]);
  });

  it("fills the commit messages when the preview arrives after the step opened", async () => {
    open(runState("Previewing", "Preview", { draft: DRAFT_CONTEXT.prefill }));
    expect(await screen.findByText("Demo Plugin")).toBeTruthy();
    act(() => {
      useRunStore.getState().receiveState(
        PROJECT_PATH,
        runState("AwaitingPublish", "Publish", {
          preview: PREVIEW,
          draft: DRAFT_CONTEXT.prefill,
        }),
      );
    });
    expect((await screen.findByLabelText<HTMLInputElement>("Trunk commit message")).value).toBe(
      "Release 1.0.1",
    );
    expect(screen.getByLabelText<HTMLInputElement>("Tag commit message").value).toBe("Tag 1.0.1");
  });

  it("hides Cancel while publishing", async () => {
    open(runState("Publishing", "Publish", { preview: PREVIEW, draft: DRAFT_CONTEXT.prefill }));
    expect(await screen.findByText("Demo Plugin")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
  });

  it("does not start a release while a package is being built", async () => {
    open(runState("DryRunComplete", null));
    act(() => {
      useRunStore.getState().setBuilding(PROJECT_PATH, true);
    });
    expect(
      (await screen.findByRole<HTMLButtonElement>("button", { name: "Release" })).disabled,
    ).toBe(true);
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Update assets" }).disabled).toBe(
      true,
    );
  });

  it("shows Starting and ignores a second click until the start answers", async () => {
    const user = userEvent.setup();
    open(runState("DryRunComplete", null));
    let answer: (state: unknown) => void = () => undefined;
    tauriMock.handle("start_run", () => new Promise((resolve) => (answer = resolve)));
    await user.click(screen.getByRole("button", { name: "Release" }));
    const starting = await screen.findByRole<HTMLButtonElement>("button", { name: "Starting…" });
    expect(starting.disabled).toBe(true);
    await user.click(starting);
    expect(tauriMock.calls.filter((c) => c.command === "start_run")).toHaveLength(1);
    await act(async () => {
      answer(runState("Idle", "Detect"));
      await Promise.resolve();
    });
  });

  it("asks before discarding an unfinished release", async () => {
    const user = userEvent.setup();
    const interrupted = summary({
      unfinished: {
        id: "20260917-090000",
        slug: "demo",
        project_path: PROJECT_PATH,
        version: "1.0.1",
        main_file: "demo.php",
        dry_run: false,
        started: "2026-09-17T09:00:00Z",
        finished: null,
        steps: [],
        revisions: { trunk: null, tag: null, assets: null },
        outcome: null,
        diffs: [],
        tag_message: null,
        verification: null,
        snapshot: null,
        discarded: false,
        assets_only: false,
      },
    });
    open(runState("Failed", "Build"), interrupted);
    await user.click(await screen.findByRole("button", { name: "Discard" }));
    const dialog = screen.getByRole("dialog", { name: "Discard the unfinished release" });
    expect(tauriMock.calls.some((c) => c.command === "discard_run")).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "Discard" }));
    expect(tauriMock.calls.find((c) => c.command === "discard_run")?.args).toEqual({
      path: PROJECT_PATH,
      runId: "20260917-090000",
    });
  });

  it("offers to check and finish a publish the server never confirmed", async () => {
    const stopped = summary({
      unfinished: {
        id: "20260917-090000",
        slug: "demo",
        project_path: PROJECT_PATH,
        version: "1.0.1",
        main_file: "demo.php",
        dry_run: false,
        started: "2026-09-17T09:00:00Z",
        finished: null,
        steps: [],
        revisions: { trunk: null, tag: null, assets: null },
        outcome: null,
        diffs: [],
        tag_message: null,
        verification: null,
        snapshot: null,
        discarded: false,
        assets_only: false,
        in_flight: { kind: "Commit", since: 41, message: "Release 1.0.1" },
      },
    });
    open(runState("Failed", "Publish"), stopped);
    expect(
      await screen.findByText(
        "SVNpush stopped while publishing 1.0.1, before WordPress.org confirmed it. Resume checks WordPress.org and finishes the release.",
      ),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Resume: check and finish" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Discard" })).toBeNull();
  });

  it("resumes an interrupted assets-only commit without promising to finish a release", async () => {
    const stopped = summary({
      unfinished: {
        id: "20260917-090000",
        slug: "demo",
        project_path: PROJECT_PATH,
        version: null,
        main_file: null,
        dry_run: false,
        started: "2026-09-17T09:00:00Z",
        finished: null,
        steps: [],
        revisions: { trunk: null, tag: null, assets: null },
        outcome: null,
        diffs: [],
        tag_message: null,
        verification: null,
        snapshot: null,
        discarded: false,
        assets_only: true,
        in_flight: { kind: "Commit", since: 41, message: "Update assets" },
      },
    });
    open(runState("Failed", "Publish"), stopped);
    expect(await screen.findByRole("button", { name: "Resume" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Resume: check and finish" })).toBeNull();
  });

  it("offers the team's pre-build command but never saves it by itself", async () => {
    const user = userEvent.setup();
    open(runState("DryRunComplete", null), summary({ team_pre_build_command: "npm run build" }));
    expect(await screen.findByText("npm run build")).toBeTruthy();
    const field = screen.getByLabelText<HTMLInputElement>("Pre-build command");
    expect(field.value).toBe("");
    await user.click(screen.getByRole("button", { name: "Use this command" }));
    expect(field.value).toBe("npm run build");
    expect(tauriMock.calls.some((c) => c.command === "update_project")).toBe(false);
    expect(screen.queryByRole("button", { name: "Use this command" })).toBeNull();
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
      assetsOnly: false,
    });
  });
  it("updates assets alone and confirms without a tag", async () => {
    const user = userEvent.setup();
    open(runState("DryRunComplete", null));
    tauriMock.handle("start_run", () => runState("Idle", "Detect", { assets_only: true }));
    await user.click(screen.getByRole("button", { name: "Update assets" }));
    expect(tauriMock.calls.find((c) => c.command === "start_run")?.args).toEqual({
      path: PROJECT_PATH,
      dryRun: false,
      assetsOnly: true,
    });
  });

  it("asks for one assets commit message and no tag message", async () => {
    const user = userEvent.setup();
    open(
      runState("AwaitingPublish", "Publish", {
        assets_only: true,
        preview: {
          ...PREVIEW,
          trunk: { added: [], modified: [], deleted: [] },
          assets: { added: ["banner-772x250.png"], modified: [], deleted: [] },
          trunk_message: "Update assets",
          tag_message: "",
          tag_url: "",
          diffs: [],
        },
      }),
    );
    expect(await screen.findByLabelText("Assets commit message")).toBeTruthy();
    expect(screen.queryByLabelText("Tag commit message")).toBeNull();
    expect(screen.queryByText("Tag to create")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Publish" }));
    const dialog = screen.getByRole("dialog", { name: "Publish these assets" });
    await user.click(within(dialog).getByRole("button", { name: "Publish" }));
    expect(tauriMock.calls.find((c) => c.command === "confirm_publish")?.args).toEqual({
      path: PROJECT_PATH,
      trunkMessage: "Update assets",
      tagMessage: "",
    });
  });
});
