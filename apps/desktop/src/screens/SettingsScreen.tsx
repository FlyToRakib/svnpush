import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState, type SyntheticEvent } from "react";
import { DoctorPanel } from "../components/DoctorPanel";
import { ErrorNotice } from "../components/ErrorNotice";
import { Modal } from "../components/Modal";
import { ScreenHeader } from "../components/ScreenHeader";
import { ThemeToggle } from "../components/ThemeToggle";
import type { AppSettings } from "../ipc/bindings/AppSettings";
import type { DoctorReport } from "../ipc/bindings/DoctorReport";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { UpdateInfo } from "../ipc/bindings/UpdateInfo";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";
import { isActive, useRunStore } from "../store/runStore";
import { S } from "../strings";

/** Theme, tool paths with Doctor, privacy, diagnostics, version and updates. */
export function SettingsScreen() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [doctor, setDoctor] = useState<DoctorReport | null>(null);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [error, setError] = useState<ErrorView | null>(null);
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [doctorRunning, setDoctorRunning] = useState(true);
  const [installing, setInstalling] = useState(false);
  // Installing restarts the app, which would cut a release off mid-way.
  const releasing = useRunStore((s) =>
    Object.values(s.runs).some((r) => isActive(r.state) || r.pending || r.building),
  );

  const attempt = async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
      setError(null);
    } catch (e) {
      setError(toErrorView(e));
    } finally {
      setBusy(false);
    }
  };

  const doctorAgain = async () => {
    setDoctorRunning(true);
    try {
      setDoctor(await commands.runDoctor());
    } finally {
      setDoctorRunning(false);
    }
  };

  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setVersion, () => undefined);
    commands.getSettings().then(setSettings, (e: unknown) => {
      setError(toErrorView(e));
    });
    commands
      .runDoctor()
      .then(setDoctor, (e: unknown) => {
        setError(toErrorView(e));
      })
      .finally(() => {
        setDoctorRunning(false);
      });
  }, []);

  const edit = (next: AppSettings) => {
    setSettings(next);
    setSaved(false);
  };

  const save = (event: SyntheticEvent) => {
    event.preventDefault();
    if (!settings) {
      return;
    }
    void attempt(async () => {
      setSettings(await commands.saveSettings(settings));
      await doctorAgain();
      setSaved(true);
    });
  };

  return (
    <div className="settings-screen stack">
      <ScreenHeader title={S.settings.title} subtitle={S.settings.subtitle} />
      {error && <ErrorNotice error={error} />}

      <section className="card">
        <div className="card__header">
          <h2 className="card__title">{S.settings.appearance}</h2>
        </div>
        <div className="card__body row">
          <span className="field__label">{S.settings.theme}</span>
          <ThemeToggle />
        </div>
      </section>

      {!settings && !error && <p className="muted">{S.common.loading}</p>}
      {settings && (
        <form className="stack" onSubmit={save}>
          <section className="card">
            <div className="card__header">
              <h2 className="card__title">{S.settings.tools}</h2>
              <button
                type="button"
                className="btn btn--sm"
                disabled={busy}
                onClick={() => {
                  void attempt(doctorAgain);
                }}
              >
                {S.settings.doctor}
              </button>
            </div>
            <div className="card__body stack">
              <div className="grid-2">
                <div className="field">
                  <label className="field__label" htmlFor="set-svn">
                    {S.settings.svnPath}
                  </label>
                  <input
                    id="set-svn"
                    className="input mono"
                    value={settings.svn_path ?? ""}
                    onChange={(e) => {
                      edit({ ...settings, svn_path: e.target.value || null });
                    }}
                  />
                  <p className="field__hint">{S.settings.pathHint}</p>
                </div>
                <div className="field">
                  <label className="field__label" htmlFor="set-git">
                    {S.settings.gitPath}
                  </label>
                  <input
                    id="set-git"
                    className="input mono"
                    value={settings.git_path ?? ""}
                    onChange={(e) => {
                      edit({ ...settings, git_path: e.target.value || null });
                    }}
                  />
                  <p className="field__hint">{S.settings.pathHint}</p>
                </div>
              </div>
              {doctorRunning && <p className="muted">{S.settings.doctorRunning}</p>}
              {doctor && <DoctorPanel report={doctor} />}
            </div>
          </section>

          <section className="card">
            <div className="card__header">
              <h2 className="card__title">{S.settings.privacy}</h2>
            </div>
            <div className="card__body stack">
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={settings.wordpress_version_lookup}
                  onChange={(e) => {
                    edit({ ...settings, wordpress_version_lookup: e.target.checked });
                  }}
                />
                {S.settings.wordpressLookup}
              </label>
              <p className="field__hint">{S.settings.wordpressLookupHint}</p>
            </div>
          </section>

          <div className="row">
            <button type="submit" className="btn btn--primary" disabled={busy}>
              {S.settings.save}
            </button>
            {saved && (
              <span className="notice notice--ok" role="status">
                {S.settings.saved}
              </span>
            )}
          </div>
        </form>
      )}

      <section className="card">
        <div className="card__header">
          <h2 className="card__title">{S.settings.diagnostics}</h2>
        </div>
        <div className="card__body stack">
          <p className="field__hint">{S.settings.diagnosticsHint}</p>
          <div className="row">
            <button
              type="button"
              className="btn"
              disabled={busy}
              onClick={() => {
                setCopied(false);
                void attempt(async () => {
                  await navigator.clipboard.writeText(await commands.diagnostics());
                  setCopied(true);
                });
              }}
            >
              {S.settings.copyDiagnostics}
            </button>
            {copied && (
              <span className="notice notice--ok" role="status">
                {S.common.copied}
              </span>
            )}
          </div>
        </div>
      </section>

      <section className="card">
        <div className="card__header">
          <h2 className="card__title">{S.settings.about}</h2>
        </div>
        <div className="card__body stack">
          {version && <p>{S.settings.version(version)}</p>}
          {update &&
            (update.available ? (
              <div className="stack">
                <div className="row">
                  <span className="notice notice--info">
                    {S.settings.updateAvailable(update.available)}
                  </span>
                  <button
                    type="button"
                    className="btn btn--primary"
                    disabled={busy || releasing}
                    onClick={() => {
                      setInstalling(true);
                    }}
                  >
                    {S.settings.installUpdate}
                  </button>
                </div>
                {releasing && <p className="field__hint">{S.settings.updateWaits}</p>}
              </div>
            ) : (
              <p className="notice notice--ok">{S.settings.upToDate}</p>
            ))}
          <div>
            <button
              type="button"
              className="btn"
              disabled={busy}
              onClick={() => {
                void attempt(async () => {
                  setUpdate(await commands.checkUpdate());
                });
              }}
            >
              {S.settings.checkUpdate}
            </button>
          </div>
        </div>
      </section>

      <Modal
        open={installing}
        title={S.settings.installTitle}
        onClose={() => {
          setInstalling(false);
        }}
        actions={
          <>
            <button
              type="button"
              className="btn"
              onClick={() => {
                setInstalling(false);
              }}
            >
              {S.common.cancel}
            </button>
            <button
              type="button"
              className="btn btn--primary"
              disabled={releasing}
              onClick={() => {
                setInstalling(false);
                void attempt(() => commands.installUpdate().then(() => undefined));
              }}
            >
              {S.settings.installUpdate}
            </button>
          </>
        }
      >
        <p>{S.settings.installBody}</p>
      </Modal>
    </div>
  );
}
