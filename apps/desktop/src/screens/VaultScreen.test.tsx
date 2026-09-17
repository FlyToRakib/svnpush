import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { tauriMock } from "../test/tauriMock";
import { VaultScreen } from "./VaultScreen";

const EMPTY = { accounts: [], keychain_problem: null };
const ONE = {
  accounts: [{ host: "plugins.svn.wordpress.org", username: "someone", has_password: true }],
  keychain_problem: null,
};

describe("VaultScreen", () => {
  it("shows the SVN password reminder and adds an account with the WordPress.org host", async () => {
    tauriMock.handle("vault_view", () => EMPTY);
    tauriMock.handle("save_account", () => ONE);
    const user = userEvent.setup();
    render(<VaultScreen />);
    expect(screen.getByText(/not your account password/)).toBeTruthy();
    await user.click(
      (await screen.findAllByRole("button", { name: "Add account" }))[0] as HTMLElement,
    );
    expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Host" }).value).toBe(
      "plugins.svn.wordpress.org",
    );
    await user.type(screen.getByRole("textbox", { name: "Username" }), "someone");
    const password = screen.getByLabelText<HTMLInputElement>("SVN password");
    expect(password.type).toBe("password");
    await user.type(password, "app-password");
    await user.click(screen.getByRole("button", { name: "Save account" }));
    expect(tauriMock.calls.find((c) => c.command === "save_account")?.args).toEqual({
      host: "plugins.svn.wordpress.org",
      username: "someone",
      password: "app-password",
    });
    expect(await screen.findByText("Password stored")).toBeTruthy();
    expect(screen.queryByDisplayValue("app-password")).toBeNull();
  });

  it("explains an unavailable keychain and offers no fallback", async () => {
    tauriMock.handle("vault_view", () => ({
      accounts: [],
      keychain_problem: {
        code: "VAULT_UNAVAILABLE",
        message: "The operating system keychain is not available.",
        fix: "Install and unlock a Secret Service keyring.",
      },
    }));
    render(<VaultScreen />);
    expect(await screen.findByText("The operating system keychain is not available.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Add account" })).toBeNull();
  });

  it("tests and removes an account after confirmation", async () => {
    tauriMock.handle("vault_view", () => ONE);
    tauriMock.handle(
      "test_account",
      () => "The keychain returned the password and plugins.svn.wordpress.org answered.",
    );
    tauriMock.handle("remove_account", () => EMPTY);
    const user = userEvent.setup();
    render(<VaultScreen />);
    await user.click(await screen.findByRole("button", { name: "Test" }));
    expect(await screen.findByText(/answered\./)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Remove" }));
    const dialog = screen.getByRole("dialog", { name: "Forget this account" });
    await user.click(within(dialog).getByRole("button", { name: "Remove" }));
    expect(tauriMock.calls.find((c) => c.command === "remove_account")?.args).toEqual({
      host: "plugins.svn.wordpress.org",
      username: "someone",
    });
  });
});
