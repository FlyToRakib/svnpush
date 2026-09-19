import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FileReview } from "../ipc/bindings/FileReview";
import type { RunState } from "../ipc/bindings/RunState";
import { useProjectStore } from "../store/projectStore";
import { useRunStore } from "../store/runStore";
import { PROJECT_PATH, runState, summary } from "../test/fixtures";
import { tauriMock } from "../test/tauriMock";
import { ReleaseScreen } from "./ReleaseScreen";

function review(overrides: Partial<FileReview> = {}): FileReview {
  return {
    reasons: ["NoDistignore", "FirstRelease"],
    editable: true,
    distignore_exists: false,
    distignore: ".*\n/docs\n",
    preview: {
      files: [
        { path: "demo.php", size: 1200 },
        { path: "includes/a.php", size: 300 },
        { path: "includes/b.php", size: 200 },
        { path: "readme.txt", size: 800 },
      ],
      excluded: [".agent/", ".git/", "docs/"],
      excluded_total: 3,
    },
    new_items: [],
    ...overrides,
  };
}

function open(state: RunState) {
  useProjectStore.setState({ projects: [summary()], selectedPath: PROJECT_PATH, loaded: true });
  useRunStore.setState({ runs: { [PROJECT_PATH]: { state, logs: [], actionError: null } } });
  tauriMock.handle("current_run", () => state);
  tauriMock.handle("project_history", () => []);
  tauriMock.handle("list_projects", () => [summary()]);
  tauriMock.handle("provider_adapters", () => []);
  tauriMock.handle("list_providers", () => ({ schema: 1, providers: [], fallback: [] }));
  render(<ReleaseScreen onOpenProviders={vi.fn()} onOpenHelp={vi.fn()} />);
}

const confirmations = () =>
  tauriMock.calls.filter((c) => c.command === "confirm_release_files").map((c) => c.args);

describe("Release file check", () => {
  beforeEach(() => {
    useRunStore.setState({ runs: {}, listening: false });
  });

  it("shows what is released and left out, and saves the proposed .distignore", async () => {
    const user = userEvent.setup();
    open(runState("AwaitingFileReview", "Build", { file_review: review() }));
    expect(await screen.findByText(/has no .distignore yet/)).toBeTruthy();
    expect(screen.getByText(/first release/)).toBeTruthy();
    expect(screen.getByText("4 file(s), 2 KB, will be released · 3 left out")).toBeTruthy();
    expect(screen.getByText("includes/")).toBeTruthy();
    expect(screen.getByText(".agent/")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Save .distignore and continue" }));
    expect(confirmations()).toEqual([{ path: PROJECT_PATH, distignore: ".*\n/docs\n" }]);
  });

  it("recalculates the lists when the rules change", async () => {
    const user = userEvent.setup();
    tauriMock.handle("preview_release_files", () => ({
      files: [{ path: "demo.php", size: 1200 }],
      excluded: [".agent/", "includes/"],
      excluded_total: 2,
    }));
    open(runState("AwaitingFileReview", "Build", { file_review: review() }));
    const rules = await screen.findByLabelText("Rules (.distignore)");
    await user.type(rules, "/includes");
    await waitFor(() => {
      expect(screen.getByText("1 file(s), 1 KB, will be released · 2 left out")).toBeTruthy();
    });
    const call = tauriMock.calls.find((c) => c.command === "preview_release_files");
    expect(call?.args?.distignore).toBe(".*\n/docs\n/includes");
  });

  it("marks new top-level items and continues without rewriting an existing file", async () => {
    const user = userEvent.setup();
    open(
      runState("AwaitingFileReview", "Build", {
        file_review: review({
          reasons: ["NewItems"],
          distignore_exists: true,
          new_items: ["includes/"],
        }),
      }),
    );
    expect(await screen.findByText("New")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "The files are right, continue" }));
    expect(confirmations()).toEqual([{ path: PROJECT_PATH, distignore: null }]);
  });
});
