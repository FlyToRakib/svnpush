import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";
import { tauriMock } from "./tauriMock";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) => tauriMock.invoke(command, args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (e: { payload: unknown }) => void) =>
    tauriMock.listen(event, handler),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => tauriMock.dialogOpen(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (url: string) => tauriMock.openUrl(url),
  revealItemInDir: (path: string) => tauriMock.reveal(path),
}));

// jsdom has no layout: give elements the methods components call.
Element.prototype.scrollIntoView = function scrollIntoView() {};
HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
  this.setAttribute("open", "");
};
HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
  this.removeAttribute("open");
  this.dispatchEvent(new Event("close"));
};

beforeEach(() => {
  tauriMock.reset();
});

afterEach(() => {
  cleanup();
  window.localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
});
