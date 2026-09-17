import { useState } from "react";
import { S } from "../strings";
import { loadTheme, saveTheme, THEME_CHOICES, type ThemeChoice } from "../theme";

const LABELS: Record<ThemeChoice, string> = {
  system: S.theme.system,
  light: S.theme.light,
  dark: S.theme.dark,
};

/** Three-way segmented control: System, Light, Dark. The choice persists. */
export function ThemeToggle() {
  const [choice, setChoice] = useState<ThemeChoice>(loadTheme);

  const choose = (next: ThemeChoice) => {
    saveTheme(next);
    setChoice(next);
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
