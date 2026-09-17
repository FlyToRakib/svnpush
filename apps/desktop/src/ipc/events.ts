import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { RunLogEvent } from "./bindings/RunLogEvent";
import type { RunStateEvent } from "./bindings/RunStateEvent";

/** Subscribes to run state snapshots. */
export function onRunState(handler: (event: RunStateEvent) => void): Promise<UnlistenFn> {
  return listen<RunStateEvent>("run-state", (event) => {
    handler(event.payload);
  });
}

/** Subscribes to run log lines. */
export function onRunLog(handler: (event: RunLogEvent) => void): Promise<UnlistenFn> {
  return listen<RunLogEvent>("run-log", (event) => {
    handler(event.payload);
  });
}
