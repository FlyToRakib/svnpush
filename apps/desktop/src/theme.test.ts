import { describe, expect, it } from "vitest";
import { applyTheme, loadTheme, saveTheme } from "./theme";

describe("theme", () => {
  it("defaults to system when nothing is stored", () => {
    expect(loadTheme()).toBe("system");
  });

  it("ignores an invalid stored value", () => {
    window.localStorage.setItem("svnpush.theme", "sepia");
    expect(loadTheme()).toBe("system");
  });

  it("persists and applies a choice", () => {
    saveTheme("dark");
    expect(loadTheme()).toBe("dark");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  });

  it("removes the attribute for system", () => {
    applyTheme("light");
    applyTheme("system");
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  });
});
