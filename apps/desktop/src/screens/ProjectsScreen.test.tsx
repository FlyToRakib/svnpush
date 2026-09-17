import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { useProjectStore } from "../store/projectStore";
import { summary } from "../test/fixtures";
import { tauriMock } from "../test/tauriMock";
import { ProjectsScreen } from "./ProjectsScreen";

describe("ProjectsScreen", () => {
  it("shows the empty state with Add project", () => {
    useProjectStore.setState({ projects: [], loaded: true, error: null, selectedPath: null });
    render(<ProjectsScreen onOpen={vi.fn()} />);
    expect(screen.getByText("No projects yet")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Add project" })).toHaveLength(2);
  });

  it("lists projects with version and last release, and opens one", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    const row = summary({
      project: {
        ...summary().project,
        last_release: { version: "1.0.0", finished: "2026-09-01T10:00:00Z", outcome: "Complete" },
      },
    });
    useProjectStore.setState({ projects: [row], loaded: true, error: null });
    render(<ProjectsScreen onOpen={onOpen} />);
    expect(screen.getByText("Published", { exact: false })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Demo Plugin" }));
    expect(onOpen).toHaveBeenCalledWith("C:/plugins/demo");
  });

  it("adds a project from a chosen folder with the suggested SVN URL", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    useProjectStore.setState({ projects: [], loaded: true, error: null });
    tauriMock.nextFolder = "C:/plugins/demo";
    tauriMock.handle("inspect_folder", () => ({
      folder: "C:/plugins/demo",
      suggested_svn_url: "https://plugins.svn.wordpress.org/demo",
      main_file_candidates: ["demo.php"],
      name: "Demo Plugin",
      version: "1.0.0",
    }));
    tauriMock.handle("add_project", () => summary());
    render(<ProjectsScreen onOpen={onOpen} />);
    await user.click(screen.getAllByRole("button", { name: "Add project" })[0] as HTMLElement);
    await user.click(screen.getByRole("button", { name: "Choose folder" }));
    expect(await screen.findByText("Demo Plugin, version 1.0.0")).toBeTruthy();
    expect(screen.getByRole<HTMLInputElement>("textbox", { name: "SVN URL" }).value).toBe(
      "https://plugins.svn.wordpress.org/demo",
    );
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(tauriMock.calls.find((c) => c.command === "add_project")?.args).toEqual({
      folder: "C:/plugins/demo",
      svnUrl: "https://plugins.svn.wordpress.org/demo",
      mainFile: null,
    });
    expect(onOpen).toHaveBeenCalledWith("C:/plugins/demo");
  });

  it("shows the reason when a folder cannot be added", async () => {
    const user = userEvent.setup();
    useProjectStore.setState({ projects: [], loaded: true, error: null });
    tauriMock.nextFolder = "C:/plugins/demo";
    tauriMock.handle("inspect_folder", () => ({
      folder: "C:/plugins/demo",
      suggested_svn_url: "https://plugins.svn.wordpress.org/demo",
      main_file_candidates: ["demo.php"],
      name: "Demo Plugin",
      version: "1.0.0",
    }));
    tauriMock.reject("add_project", {
      code: "PROJECT_EXISTS",
      message: "Demo Plugin is already a project.",
      fix: "Open it from the Projects list.",
    });
    render(<ProjectsScreen onOpen={vi.fn()} />);
    await user.click(screen.getAllByRole("button", { name: "Add project" })[0] as HTMLElement);
    await user.click(screen.getByRole("button", { name: "Choose folder" }));
    await screen.findByText("Demo Plugin, version 1.0.0");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Demo Plugin is already a project.")).toBeTruthy();
  });
});
