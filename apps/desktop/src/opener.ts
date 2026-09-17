import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";

/** Shows a file in the system file manager. */
export function openPath(path: string): Promise<void> {
  return revealItemInDir(path);
}

/** Opens a WordPress.org URL in the default browser (the only host the capability allows). */
export function openExternal(url: string): Promise<void> {
  return openUrl(url);
}
