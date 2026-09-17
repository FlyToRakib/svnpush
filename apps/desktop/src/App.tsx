import { useState } from "react";
import { TitleBar } from "./components/TitleBar";
import { ProjectsScreen } from "./screens/ProjectsScreen";
import { ProvidersScreen } from "./screens/ProvidersScreen";
import { ReleaseScreen } from "./screens/ReleaseScreen";
import type { Screen } from "./screens/screen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { VaultScreen } from "./screens/VaultScreen";

function renderScreen(screen: Screen) {
  switch (screen) {
    case "projects":
      return <ProjectsScreen />;
    case "release":
      return <ReleaseScreen />;
    case "providers":
      return <ProvidersScreen />;
    case "vault":
      return <VaultScreen />;
    case "settings":
      return <SettingsScreen />;
  }
}

/** The application frame: title bar and the current screen. */
export function App() {
  const [screen, setScreen] = useState<Screen>("projects");

  return (
    <div className="app">
      <TitleBar current={screen} onNavigate={setScreen} />
      <main className="app__main">
        <div className="app__content">{renderScreen(screen)}</div>
      </main>
    </div>
  );
}
