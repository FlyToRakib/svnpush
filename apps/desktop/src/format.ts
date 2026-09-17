import type { Delta } from "./ipc/bindings/Delta";

/** `12 B`, `2 KB`, `1.5 MB`. */
export function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  if (bytes >= 1024) {
    return `${Math.round(bytes / 1024).toString()} KB`;
  }
  return `${bytes.toString()} B`;
}

/** An ISO 8601 UTC time in the viewer's locale, date and time. */
export function formatDateTime(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
}

/** `+2 ~1 −0` */
export function formatDelta(delta: Delta): string {
  return `+${delta.added.length.toString()} ~${delta.modified.length.toString()} −${delta.deleted.length.toString()}`;
}

/** The run id `20260917-101530` as a date. */
export function runIdToIso(id: string): string {
  const m = /^(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})$/.exec(id);
  return m
    ? `${m[1] ?? ""}-${m[2] ?? ""}-${m[3] ?? ""}T${m[4] ?? ""}:${m[5] ?? ""}:${m[6] ?? ""}Z`
    : id;
}
