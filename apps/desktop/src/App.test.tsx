import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";
import { tauriMock } from "./test/tauriMock";

const TOOL_OK = {
  kind: "Svn",
  path: "/usr/bin/svn",
  version: "1.14.5",
  ok: true,
  message: "Subversion 1.14.5.",
  fix: null,
};

function mockApp(setupSeen: boolean) {
  tauriMock.handle("list_projects", () => []);
  tauriMock.handle("provider_adapters", () => []);
  tauriMock.handle("list_providers", () => ({ schema: 1, providers: [], fallback: [] }));
  tauriMock.handle("vault_view", () => ({ accounts: [], keychain_problem: null }));
  tauriMock.handle("run_doctor", () => ({
    svn: TOOL_OK,
    git: { ...TOOL_OK, kind: "Git" },
    keychain_problem: null,
  }));
  tauriMock.handle("svn_install_plan", () => ({
    method: "Winget",
    command: "winget install --id Slik.Subversion",
  }));
  tauriMock.handle("save_settings", (args) => args?.settings);
  tauriMock.handle("get_settings", () => ({
    schema: 1,
    svn_path: null,
    git_path: null,
    wordpress_version_lookup: true,
    privacy_notice_seen: [],
    setup_seen: setupSeen,
  }));
}

describe("App", () => {
  it("navigates between the screens", async () => {
    mockApp(true);
    const user = userEvent.setup();
    render(<App />);
    expect(screen.getByRole("heading", { name: "Projects" })).toBeTruthy();

    for (const [link, heading] of [
      ["Release", "Release"],
      ["Providers", "AI Providers"],
      ["Vault", "Vault"],
      ["Settings", "Settings"],
      ["Help", "Help"],
    ] as const) {
      await user.click(screen.getByRole("button", { name: link }));
      expect(screen.getByRole("heading", { name: heading, level: 1 })).toBeTruthy();
    }
  });

  it("opens Help with the setup checklist on the first launch, once", async () => {
    mockApp(false);
    render(<App />);
    expect(await screen.findByText(/Welcome to SVNpush/)).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Help", level: 1 })).toBeTruthy();
    await waitFor(() => {
      const saved = tauriMock.calls.find((c) => c.command === "save_settings");
      expect((saved?.args?.settings as { setup_seen: boolean }).setup_seen).toBe(true);
    });
  });
});
