import { useEffect, useRef, useState } from "react";
import { Checklist } from "../components/Checklist";
import { ErrorNotice } from "../components/ErrorNotice";
import { LogDrawer } from "../components/LogDrawer";
import { Modal } from "../components/Modal";
import { PastReleases } from "../components/PastReleases";
import { ProjectSettingsForm } from "../components/ProjectSettingsForm";
import { ScreenHeader } from "../components/ScreenHeader";
import { UnfinishedBanner } from "../components/UnfinishedBanner";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { RunJournal } from "../ipc/bindings/RunJournal";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";
import { openExternal } from "../opener";
import { useProjectStore } from "../store/projectStore";
import { isActive, useRunStore } from "../store/runStore";
import { S } from "../strings";

const NO_LOGS: never[] = [];

/** The project page: header, Release and Dry run, the checklist, the log and past releases. */
export function ReleaseScreen() {
  const { projects, selectedPath, load: loadProjects, update, remove, select } = useProjectStore();
  const summary = projects.find((p) => p.project.path === selectedPath);
  const path = summary?.project.path ?? "";
  const run = useRunStore((s) => s.runs[path]);
  const runs = {
    load: useRunStore((s) => s.load),
    start: useRunStore((s) => s.start),
    approve: useRunStore((s) => s.approve),
    publish: useRunStore((s) => s.publish),
    cancel: useRunStore((s) => s.cancel),
    resume: useRunStore((s) => s.resume),
    discard: useRunStore((s) => s.discard),
  };
  const loadRun = runs.load;
  const [dryRun, setDryRun] = useState(false);
  const [history, setHistory] = useState<RunJournal[]>([]);
  const [removing, setRemoving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [localError, setLocalError] = useState<ErrorView | null>(null);
  const opened = useRef<string | null>(null);

  const state = run?.state ?? null;
  const active = isActive(state);
  const publishResult = state?.publish ?? null;
  const runId = state?.id ?? null;

  // Load the current run and history on open, and again whenever a run ends.
  useEffect(() => {
    if (!path) {
      return;
    }
    void loadRun(path);
    commands.projectHistory(path).then(setHistory, (e: unknown) => {
      setLocalError(toErrorView(e));
    });
    if (!active) {
      void loadProjects();
    }
  }, [path, active, loadRun, loadProjects]);

  // Open the plugin page once per successful publish, when the project asks for it.
  useEffect(() => {
    if (runId && publishResult?.open_plugin_page && opened.current !== runId) {
      opened.current = runId;
      void openExternal(publishResult.plugin_url);
    }
  }, [runId, publishResult]);

  if (!summary) {
    return (
      <>
        <ScreenHeader title={S.release.title} subtitle={S.release.subtitle} />
        <div className="card">
          <div className="empty">
            <h2 className="empty__title">{S.release.emptyTitle}</h2>
            <p className="empty__body">{S.release.emptyBody}</p>
          </div>
        </div>
      </>
    );
  }

  const { project } = summary;
  const unfinished = !active ? summary.unfinished : null;

  const resetWorkingCopy = async () => {
    try {
      await commands.resetWorkingCopy(path);
      setNotice(S.release.resetDone);
      setLocalError(null);
    } catch (e) {
      setLocalError(toErrorView(e));
    }
  };

  return (
    <div className="release">
      <ScreenHeader
        title={project.name}
        subtitle={state ? S.release.phase[state.phase] : project.slug}
        actions={
          <div className="release__actions">
            {active ? (
              <button
                type="button"
                className="btn btn--danger"
                onClick={() => {
                  void runs.cancel(path);
                }}
              >
                {S.release.cancel}
              </button>
            ) : (
              <>
                <label className="checkbox" title={S.release.dryRunHint}>
                  <input
                    type="checkbox"
                    checked={dryRun}
                    onChange={(e) => {
                      setDryRun(e.target.checked);
                    }}
                  />
                  {S.release.dryRun}
                </label>
                <button
                  type="button"
                  className="btn btn--primary"
                  disabled={summary.locked || Boolean(unfinished)}
                  onClick={() => {
                    setNotice(null);
                    void runs.start(path, dryRun);
                  }}
                >
                  {dryRun ? S.release.dryRun : S.release.release}
                </button>
              </>
            )}
          </div>
        }
      />

      <dl className="facts release__facts">
        <dt>{S.release.version}</dt>
        <dd className="mono">{summary.version ?? "—"}</dd>
        <dt>{S.release.svnUrl}</dt>
        <dd className="mono break">{project.svn_url}</dd>
        <dt>{S.release.account}</dt>
        <dd className="mono">{summary.account ?? S.release.noAccount}</dd>
      </dl>

      {summary.problem && <ErrorNotice error={summary.problem} />}
      {unfinished && (
        <UnfinishedBanner
          journal={unfinished}
          disabled={active}
          onResume={() => {
            void runs.resume(path, unfinished.id).then(loadProjects);
          }}
          onDiscard={() => {
            void runs.discard(path, unfinished.id).then(loadProjects);
          }}
        />
      )}
      {run?.actionError && <ErrorNotice error={run.actionError} />}
      {localError && <ErrorNotice error={localError} />}
      {state?.error && <ErrorNotice error={state.error} />}
      {state?.error?.code === "SVN_WORKING_COPY" ||
      state?.error?.code === "SVN_OUT_OF_DATE" ||
      state?.checks.some((c) => c.id === "V15" && c.status === "Fail") ? (
        <div className="row">
          <button
            type="button"
            className="btn btn--sm"
            disabled={active}
            onClick={() => {
              void resetWorkingCopy();
            }}
          >
            {S.release.resetWorkingCopy}
          </button>
        </div>
      ) : null}
      {notice && <p className="notice notice--ok">{notice}</p>}
      {state && state.notices.length > 0 && (
        <ul className="notice notice--info notices" aria-label={S.release.notices}>
          {state.notices.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}

      {state && (
        <Checklist
          state={state}
          disabled={!active}
          onApprove={(draft) => {
            void runs.approve(path, draft);
          }}
          onPublish={(trunk, tag) => {
            void runs.publish(path, trunk, tag);
          }}
        />
      )}

      <ProjectSettingsForm
        key={project.path}
        project={project}
        disabled={active}
        onSave={(svnUrl, settings) => update(path, svnUrl, settings)}
        onRemove={() => {
          setRemoving(true);
        }}
      />

      <PastReleases journals={history} />

      <LogDrawer logs={run?.logs ?? NO_LOGS} />

      <Modal
        open={removing}
        title={S.projectSettings.removeTitle}
        onClose={() => {
          setRemoving(false);
        }}
        actions={
          <>
            <button
              type="button"
              className="btn"
              onClick={() => {
                setRemoving(false);
              }}
            >
              {S.common.cancel}
            </button>
            <button
              type="button"
              className="btn btn--danger"
              onClick={() => {
                setRemoving(false);
                void remove(path).then(() => {
                  select(null);
                });
              }}
            >
              {S.common.remove}
            </button>
          </>
        }
      >
        <p>{S.projectSettings.removeBody}</p>
      </Modal>
    </div>
  );
}
