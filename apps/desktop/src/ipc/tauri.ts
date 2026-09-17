import { invoke } from "@tauri-apps/api/core";
import { S } from "../strings";
import type { ErrorView } from "./bindings/ErrorView";

/** A command's `{ code, message, fix }` rejection, as a throwable error. */
export class CommandError extends Error {
  readonly code: string;
  readonly fix: string | null;

  constructor(view: ErrorView) {
    super(view.message);
    this.name = "CommandError";
    this.code = view.code;
    this.fix = view.fix;
  }
}

function isErrorView(value: unknown): value is ErrorView {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof (value as { code: unknown }).code === "string" &&
    typeof (value as { message: unknown }).message === "string"
  );
}

/** Turns anything a command rejects with into an ErrorView. */
export function toErrorView(error: unknown): ErrorView {
  if (error instanceof CommandError) {
    return { code: error.code, message: error.message, fix: error.fix };
  }
  if (isErrorView(error)) {
    return { code: error.code, message: error.message, fix: error.fix ?? null };
  }
  const message = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  return { code: "UNEXPECTED", message: message || S.errors.unexpected, fix: null };
}

/** Calls a Tauri command; rejects with a CommandError. */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw new CommandError(toErrorView(error));
  }
}
