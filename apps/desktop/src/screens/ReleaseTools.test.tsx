import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useProjectStore } from "../store/projectStore";
import { useRunStore } from "../store/runStore";
import { PROJECT_PATH, summary } from "../test/fixtures";
import { tauriMock } from "../test/tauriMock";
import { ReleaseScreen } from "./ReleaseScreen";

function open() {
  useProjectStore.setState({ projects: [summary()], selectedPath: PROJECT_PATH, loaded: true });
  useRunStore.setState({ runs: {}, listening: false });
  tauriMock.handle("current_run", () => null);
  tauriMock.handle("project_history", () => []);
  tauriMock.handle("list_projects", () => [summary()]);
  tauriMock.handle("provider_adapters", () => []);
  tauriMock.handle("list_providers", () => ({ schema: 1, providers: [], fallback: [] }));
  render(<ReleaseScreen onOpenProviders={vi.fn()} onOpenHelp={vi.fn()} />);
}

describe("Project tools", () => {
  beforeEach(() => {
    tauriMock.reset();
  });

  it("checks the readme with WordPress.org's rules and links the official validator", async () => {
    tauriMock.handle("check_readme", () => ({
      issues: [
        {
          level: "Error",
          code: "invalid_license",
          message: "The License field appears to be invalid.",
        },
        {
          level: "Warning",
          code: "stable_tag_invalid",
          message: "The Stable tag field is missing or invalid.",
        },
        { level: "Note", code: "faq_missing", message: "No FAQ section was found." },
      ],
      official_url: "https://wordpress.org/plugins/developers/readme-validator/",
    }));
    const user = userEvent.setup();
    open();
    await user.click(await screen.findByRole("button", { name: "Check readme.txt" }));
    expect(await screen.findByText("1 error(s), 1 warning(s), 1 note(s)")).toBeTruthy();
    expect(screen.getByText("The License field appears to be invalid.")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Open the official validator" }));
    expect(tauriMock.opened).toEqual([
      "https://wordpress.org/plugins/developers/readme-validator/",
    ]);
  });

  it("says so when the readme has no problems", async () => {
    tauriMock.handle("check_readme", () => ({
      issues: [],
      official_url: "https://wordpress.org/x",
    }));
    const user = userEvent.setup();
    open();
    await user.click(await screen.findByRole("button", { name: "Check readme.txt" }));
    expect(await screen.findByText(/No problems found/)).toBeTruthy();
  });

  it("builds the package without releasing and shows the zip", async () => {
    tauriMock.handle("build_package", () => ({
      package: {
        root: "C:/app/builds/demo/1.0.0/demo",
        zip_path: "C:/app/builds/demo/1.0.0/demo.zip",
        sha256: "abc123",
        files: [{ rel: "demo.php", size: 1200 }],
        total_size: 1200,
        long_paths: [],
        exclusion_source: "Distignore",
      },
      checks: [],
      blocked: false,
    }));
    const user = userEvent.setup();
    open();
    await user.click(await screen.findByRole("button", { name: "Build package" }));
    expect(await screen.findByText(/Built 1 file\(s\). Nothing was published./)).toBeTruthy();
    expect(screen.getByText("C:/app/builds/demo/1.0.0/demo.zip")).toBeTruthy();
    expect(tauriMock.calls.find((c) => c.command === "build_package")?.args).toEqual({
      path: PROJECT_PATH,
    });
  });
});
