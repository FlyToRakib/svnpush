import { useEffect, useState } from "react";
import { TitleBar } from "./components/TitleBar";
import { ProjectsScreen } from "./screens/ProjectsScreen";
import { ProvidersScreen } from "./screens/ProvidersScreen";
import { ReleaseScreen } from "./screens/ReleaseScreen";
import type { Screen } from "./screens/screen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { VaultScreen } from "./screens/VaultScreen";
import { useProjectStore } from "./store/projectStore";
import { useRunStore } from "./store/runStore";

/** The application frame: title bar and the current screen. */
export function App() {
  const [screen, setScreen] = useState<Screen>("projects");
  const loadProjects = useProjectStore((s) => s.load);
  const selectProject = useProjectStore((s) => s.select);
  const listen = useRunStore((s) => s.listen);

  useEffect(() => {
    void listen();
    void loadProjects();
  }, [listen, loadProjects]);

  const openProject = (path: string) => {
    selectProject(path);
    setScreen("release");
  };

  const render = () => {
    switch (screen) {
      case "projects":
        return <ProjectsScreen onOpen={openProject} />;
      case "release":
        return (
          <ReleaseScreen
            onOpenProviders={() => {
              setScreen("providers");
            }}
          />
        );
      case "providers":
        return <ProvidersScreen />;
      case "vault":
        return <VaultScreen />;
      case "settings":
        return <SettingsScreen />;
    }
  };

  return (
    <div className="app">
      <TitleBar current={screen} onNavigate={setScreen} />
      <main className="app__main">
        <div className="app__content">{render()}</div>
      </main>
    </div>
  );
}
