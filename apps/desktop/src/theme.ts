/** The three-way theme choice. `system` follows the operating system. */
export type ThemeChoice = "system" | "light" | "dark";

export const THEME_CHOICES: readonly ThemeChoice[] = ["system", "light", "dark"];

const STORAGE_KEY = "svnpush.theme";
const CHANGE_EVENT = "svnpush-theme-change";

/** Calls `listener` whenever any toggle saves a new choice; returns the unsubscribe. */
export function subscribeTheme(listener: () => void): () => void {
  window.addEventListener(CHANGE_EVENT, listener);
  return () => {
    window.removeEventListener(CHANGE_EVENT, listener);
  };
}

function isThemeChoice(value: unknown): value is ThemeChoice {
  return value === "system" || value === "light" || value === "dark";
}

/** The persisted choice, or `system` when nothing valid is stored. */
export function loadTheme(storage: Storage = window.localStorage): ThemeChoice {
  const stored = storage.getItem(STORAGE_KEY);
  return isThemeChoice(stored) ? stored : "system";
}

/** Persists the choice and applies it to the document. */
export function saveTheme(choice: ThemeChoice, storage: Storage = window.localStorage): void {
  storage.setItem(STORAGE_KEY, choice);
  applyTheme(choice);
  window.dispatchEvent(new Event(CHANGE_EVENT));
}

/**
 * Sets `data-theme` on the root element. System removes the attribute so the
 * `prefers-color-scheme` media query decides.
 */
export function applyTheme(
  choice: ThemeChoice,
  root: HTMLElement = document.documentElement,
): void {
  if (choice === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", choice);
  }
}
