import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { tauriMock } from "../test/tauriMock";
import { SettingsScreen } from "./SettingsScreen";

vi.mock("@tauri-apps/api/app", () => ({ getVersion: () => Promise.resolve("0.1.0") }));

const SETTINGS = {
  schema: 1,
  svn_path: null,
  git_path: null,
  wordpress_version_lookup: true,
  privacy_notice_seen: [],
  setup_seen: true,
};

const DOCTOR = {
  svn: {
    kind: "Svn",
    path: "C:/svn.exe",
    version: "1.14.5",
    ok: true,
    message: "Subversion 1.14.5 at C:/svn.exe.",
    fix: null,
  },
  git: {
    kind: "Git",
    path: null,
    version: null,
    ok: false,
    message: "Git was not found.",
    fix: "Run winget install Git.Git.",
  },
  keychain_problem: null,
};

describe("SettingsScreen", () => {
  it("shows Doctor results with fixes and saves the privacy toggle", async () => {
    tauriMock.handle("get_settings", () => SETTINGS);
    tauriMock.handle("run_doctor", () => DOCTOR);
    tauriMock.handle("save_settings", (args) => args?.settings);
    const user = userEvent.setup();
    render(<SettingsScreen />);
    expect(await screen.findByText("Subversion 1.14.5 at C:/svn.exe.")).toBeTruthy();
    expect(screen.getByText("Run winget install Git.Git.")).toBeTruthy();
    expect(await screen.findByText("SVNpush 0.1.0")).toBeTruthy();
    await user.click(screen.getByRole("checkbox", { name: /api\.wordpress\.org/ }));
    await user.click(screen.getByRole("button", { name: "Save settings" }));
    const saved = tauriMock.calls.find((c) => c.command === "save_settings")?.args
      ?.settings as typeof SETTINGS;
    expect(saved.wordpress_version_lookup).toBe(false);
    expect(await screen.findByText("Settings saved.")).toBeTruthy();
  });

  it("copies redacted diagnostics and checks for updates", async () => {
    tauriMock.handle("get_settings", () => SETTINGS);
    tauriMock.handle("run_doctor", () => DOCTOR);
    tauriMock.handle("diagnostics", () => "SVNpush 0.1.0\nDoctor");
    tauriMock.handle("check_update", () => ({ current: "0.1.0", available: "0.2.0" }));
    // userEvent installs its own clipboard on setup, so replace it afterwards.
    const user = userEvent.setup();
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<SettingsScreen />);
    await user.click(await screen.findByRole("button", { name: "Copy diagnostics" }));
    expect(writeText).toHaveBeenCalledWith("SVNpush 0.1.0\nDoctor");
    await user.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Version 0.2.0 is available.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Install and restart" })).toBeTruthy();
  });
});
