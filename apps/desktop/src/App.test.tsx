import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";
import { tauriMock } from "./test/tauriMock";

describe("App", () => {
  it("navigates between the five screens", async () => {
    tauriMock.handle("list_projects", () => []);
    tauriMock.handle("provider_adapters", () => []);
    tauriMock.handle("list_providers", () => ({ schema: 1, providers: [], fallback: [] }));
    tauriMock.handle("vault_view", () => ({ accounts: [], keychain_problem: null }));
    tauriMock.handle("get_settings", () => ({
      schema: 1,
      svn_path: null,
      git_path: null,
      wordpress_version_lookup: true,
      privacy_notice_seen: [],
    }));
    const user = userEvent.setup();
    render(<App />);
    expect(screen.getByRole("heading", { name: "Projects" })).toBeTruthy();

    for (const [link, heading] of [
      ["Release", "Release"],
      ["Providers", "AI Providers"],
      ["Vault", "Vault"],
      ["Settings", "Settings"],
    ] as const) {
      await user.click(screen.getByRole("button", { name: link }));
      expect(screen.getByRole("heading", { name: heading, level: 1 })).toBeTruthy();
    }
  });
});
