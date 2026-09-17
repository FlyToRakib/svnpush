import { S } from "../strings";
import { SCREENS, type Screen } from "../screens/screen";
import { ThemeToggle } from "./ThemeToggle";

interface TitleBarProps {
  current: Screen;
  onNavigate: (screen: Screen) => void;
}

const LABELS: Record<Screen, string> = {
  projects: S.nav.projects,
  release: S.nav.release,
  providers: S.nav.providers,
  vault: S.nav.vault,
  settings: S.nav.settings,
};

/** App title, the five-screen navigation and the theme toggle. */
export function TitleBar({ current, onNavigate }: TitleBarProps) {
  return (
    <header className="titlebar">
      <span className="titlebar__brand">{S.app.name}</span>
      <nav className="titlebar__nav" aria-label={S.nav.label}>
        {SCREENS.map((screen) => (
          <button
            key={screen}
            type="button"
            className={`titlebar__link${current === screen ? " titlebar__link--active" : ""}`}
            aria-current={current === screen ? "page" : undefined}
            onClick={() => {
              onNavigate(screen);
            }}
          >
            {LABELS[screen]}
          </button>
        ))}
      </nav>
      <ThemeToggle />
    </header>
  );
}
