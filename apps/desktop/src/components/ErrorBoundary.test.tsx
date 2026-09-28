import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

function Broken(): never {
  throw new Error("Cannot read properties of undefined (reading 'files')");
}

describe("ErrorBoundary", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("shows the error and a Reload button instead of a blank window", () => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(
      <ErrorBoundary>
        <Broken />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert").textContent).toContain(
      "Cannot read properties of undefined (reading 'files')",
    );
    expect(screen.getByRole("alert").textContent).toContain("A release in progress keeps running.");
    expect(screen.getByRole("button", { name: "Reload" })).toBeTruthy();
    expect(logged).toHaveBeenCalledWith(
      "A screen failed to render",
      expect.any(Error),
      expect.anything(),
    );
  });

  it("renders its children when nothing fails", () => {
    render(
      <ErrorBoundary>
        <p>Projects</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("Projects")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
