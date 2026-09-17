import { useSyncExternalStore } from "react";
import { S } from "../strings";
import { loadTheme, saveTheme, subscribeTheme, THEME_CHOICES, type ThemeChoice } from "../theme";

const LABELS: Record<ThemeChoice, string> = {
  system: S.theme.system,
  light: S.theme.light,
  dark: S.theme.dark,
};

const currentTheme = () => loadTheme();

/** Three-way segmented control: System, Light, Dark. The choice persists, and every toggle shows it. */
export function ThemeToggle() {
  const choice = useSyncExternalStore(subscribeTheme, currentTheme);

  const choose = (next: ThemeChoice) => {
    saveTheme(next);
  };

  return (
    <div className="segmented" role="radiogroup" aria-label={S.theme.label}>
      {THEME_CHOICES.map((option) => (
        <button
          key={option}
          type="button"
          role="radio"
          aria-checked={choice === option}
          className={`segmented__option${choice === option ? " segmented__option--active" : ""}`}
          onClick={() => {
            choose(option);
          }}
        >
          {LABELS[option]}
        </button>
      ))}
    </div>
  );
}
