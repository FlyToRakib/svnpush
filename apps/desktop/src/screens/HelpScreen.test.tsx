import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { tauriMock } from "../test/tauriMock";
import { HelpScreen } from "./HelpScreen";

const MISSING = {
  kind: "Svn",
  path: null,
  version: null,
  ok: false,
  message: "Subversion was not found.",
  fix: "Install it.",
};
const FOUND = {
  kind: "Svn",
  path: "C:/Program Files/SlikSvn/bin/svn.exe",
  version: "1.14.2",
  ok: true,
  message: "Subversion 1.14.2 at C:/Program Files/SlikSvn/bin/svn.exe.",
  fix: null,
};

function mockHelp(svnFound: () => boolean, method: "Winget" | "Manual" = "Winget") {
  tauriMock.handle("run_doctor", () => ({
    svn: svnFound() ? FOUND : MISSING,
    git: { ...FOUND, kind: "Git" },
    keychain_problem: null,
  }));
  tauriMock.handle("vault_view", () => ({ accounts: [], keychain_problem: null }));
  tauriMock.handle("list_providers", () => ({ schema: 1, providers: [], fallback: [] }));
  tauriMock.handle("list_projects", () => []);
  tauriMock.handle("svn_install_plan", () => ({
    method,
    command:
      method === "Winget"
        ? "winget install --id Slik.Subversion --exact --source winget"
        : "sudo apt install subversion",
  }));
}

describe("HelpScreen", () => {
  it("installs Subversion with one click and shows it as ready", async () => {
    let installed = false;
    mockHelp(() => installed);
    tauriMock.handle("install_svn", () => {
      installed = true;
      return { ok: true, detail: "Successfully installed" };
    });
    const user = userEvent.setup();
    render(<HelpScreen welcome={false} onNavigate={vi.fn()} />);

    expect(await screen.findByText("Subversion is not installed yet.")).toBeTruthy();
    expect(screen.getByText(/--id Slik.Subversion/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Install Subversion" }));
    expect(await screen.findByText("Subversion is installed.")).toBeTruthy();
    const svnRow = screen.getByText("Subversion (svn)").closest("li") as HTMLElement;
    expect(await within(svnRow).findByText("Ready")).toBeTruthy();
  });

  it("shows the command to run where SVNpush cannot install it", async () => {
    mockHelp(() => false, "Manual");
    render(<HelpScreen welcome={false} onNavigate={vi.fn()} />);
    expect(await screen.findByText("sudo apt install subversion")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Install Subversion" })).toBeNull();
    expect(screen.getByRole("button", { name: "Check again" })).toBeTruthy();
  });

  it("links each missing item to the screen that fixes it", async () => {
    mockHelp(() => true);
    const onNavigate = vi.fn();
    const user = userEvent.setup();
    render(<HelpScreen welcome={false} onNavigate={onNavigate} />);
    await user.click(await screen.findByRole("button", { name: "Open Vault" }));
    await user.click(screen.getByRole("button", { name: "Open Projects" }));
    expect(onNavigate.mock.calls).toEqual([["vault"], ["projects"]]);
    expect(screen.getByText(/not your GitHub password/)).toBeTruthy();
  });
});
