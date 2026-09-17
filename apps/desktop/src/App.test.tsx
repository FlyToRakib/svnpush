import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
  it("navigates between the five screens", async () => {
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
      expect(screen.getByRole("heading", { name: heading })).toBeTruthy();
    }
  });
});
