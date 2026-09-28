import { useEffect, useState } from "react";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Modal } from "./components/Modal";
import { TitleBar } from "./components/TitleBar";
import { commands } from "./ipc/commands";
import { onCloseBlocked } from "./ipc/events";
import { HelpScreen } from "./screens/HelpScreen";
import { ProjectsScreen } from "./screens/ProjectsScreen";
import { ProvidersScreen } from "./screens/ProvidersScreen";
import { ReleaseScreen } from "./screens/ReleaseScreen";
import type { Screen } from "./screens/screen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { VaultScreen } from "./screens/VaultScreen";
import { useProjectStore } from "./store/projectStore";
import { useRunStore } from "./store/runStore";
import { S } from "./strings";

/** The application frame: title bar and the current screen. */
export function App() {
  const [screen, setScreen] = useState<Screen>("projects");
  const [welcome, setWelcome] = useState(false);
  const [closeBlocked, setCloseBlocked] = useState(false);
  const loadProjects = useProjectStore((s) => s.load);
  const selectProject = useProjectStore((s) => s.select);
  const listen = useRunStore((s) => s.listen);

  useEffect(() => {
    void listen();
    void loadProjects();
  }, [listen, loadProjects]);

  // The shell holds back a close while a release runs and asks here first.
  useEffect(() => {
    const unlisten = onCloseBlocked(() => {
      setCloseBlocked(true);
    });
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, []);

  // The first launch opens Help with the setup checklist, once.
  useEffect(() => {
    commands.getSettings().then(
      (settings) => {
        if (!settings.setup_seen) {
          setWelcome(true);
          setScreen("help");
          // If this fails, Help simply opens again next time.
          commands.saveSettings({ ...settings, setup_seen: true }).catch((error: unknown) => {
            console.error("Could not record that setup was seen", error);
          });
        }
      },
      () => undefined,
    );
  }, []);

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
            onOpenHelp={() => {
              setScreen("help");
            }}
          />
        );
      case "providers":
        return <ProvidersScreen />;
      case "vault":
        return <VaultScreen />;
      case "settings":
        return <SettingsScreen />;
      case "help":
        return <HelpScreen welcome={welcome} onNavigate={setScreen} />;
    }
  };

  return (
    <div className="app">
      <TitleBar current={screen} onNavigate={setScreen} />
      <main className="app__main">
        <div className="app__content">
          {/* Keyed on the screen, so opening another screen clears a render error. */}
          <ErrorBoundary key={screen}>{render()}</ErrorBoundary>
        </div>
      </main>
      <Modal
        open={closeBlocked}
        title={S.closeGuard.title}
        onClose={() => {
          setCloseBlocked(false);
        }}
        actions={
          <>
            <button
              type="button"
              className="btn"
              onClick={() => {
                setCloseBlocked(false);
              }}
            >
              {S.common.cancel}
            </button>
            <button
              type="button"
              className="btn btn--danger"
              onClick={() => {
                void commands.forceClose().catch(() => undefined);
              }}
            >
              {S.closeGuard.confirm}
            </button>
          </>
        }
      >
        <p>{S.closeGuard.body}</p>
      </Modal>
    </div>
  );
}
