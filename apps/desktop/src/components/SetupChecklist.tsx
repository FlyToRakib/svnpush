import { useCallback, useEffect, useState } from "react";
import type { DoctorReport } from "../ipc/bindings/DoctorReport";
import type { InstallOutcome } from "../ipc/bindings/InstallOutcome";
import type { InstallPlan } from "../ipc/bindings/InstallPlan";
import { commands } from "../ipc/commands";
import type { Screen } from "../screens/screen";
import { S } from "../strings";
import { SetupRow } from "./SetupRow";

interface SetupChecklistProps {
  onNavigate: (screen: Screen) => void;
}

/** Everything the checklist shows, read in one go. */
const load = () =>
  Promise.all([
    commands.runDoctor(),
    commands.vaultView(),
    commands.listProviders(),
    commands.listProjects(),
    commands.svnInstallPlan(),
  ]);

interface Counts {
  accounts: number;
  providers: number;
  projects: number;
}

/** What a new user needs before the first release, with a way to fix each item. */
export function SetupChecklist({ onNavigate }: SetupChecklistProps) {
  const [doctor, setDoctor] = useState<DoctorReport | null>(null);
  const [plan, setPlan] = useState<InstallPlan | null>(null);
  const [counts, setCounts] = useState<Counts>({ accounts: 0, providers: 0, projects: 0 });
  const [installing, setInstalling] = useState(false);
  const [outcome, setOutcome] = useState<InstallOutcome | null>(null);

  const apply = useCallback(
    ([report, vault, providers, projects, install]: Awaited<ReturnType<typeof load>>) => {
      setDoctor(report);
      setPlan(install);
      setCounts({
        accounts: vault.accounts.length,
        providers: providers.providers.length,
        projects: projects.length,
      });
    },
    [],
  );

  const refresh = async () => {
    apply(await load());
  };

  useEffect(() => {
    let live = true;
    void load().then((data) => {
      if (live) {
        apply(data);
      }
    });
    return () => {
      live = false;
    };
  }, [apply]);

  const install = async () => {
    setInstalling(true);
    setOutcome(null);
    const result = await commands.installSvn();
    setOutcome(result);
    setInstalling(false);
    await refresh();
  };

  const svnReady = doctor?.svn.ok ?? false;
  const canInstall = plan !== null && plan.method !== "Manual";

  return (
    <ol className="setup">
      <SetupRow title={S.help.svn.title} status={svnReady ? "ready" : "todo"}>
        <p className="muted">{S.help.svn.why}</p>
        {outcome && (
          <div className={`notice ${outcome.ok ? "notice--ok" : "notice--error"}`} role="status">
            <p>{outcome.ok ? S.help.svn.installed : S.help.svn.failed}</p>
            {outcome.detail && <pre className="prose-block">{outcome.detail}</pre>}
          </div>
        )}
        {svnReady ? (
          <p className="mono">{doctor?.svn.message}</p>
        ) : (
          doctor && (
            <>
              <p>{S.help.svn.missing}</p>
              {plan && (
                <p>
                  {canInstall ? S.help.svn.willRun : S.help.svn.runYourself}{" "}
                  <code className="mono">{plan.command}</code>
                </p>
              )}
              {installing && <p aria-live="polite">{S.help.svn.installing}</p>}
              <div className="row">
                {canInstall && (
                  <button
                    type="button"
                    className="btn btn--primary"
                    disabled={installing}
                    onClick={() => {
                      void install();
                    }}
                  >
                    {S.help.svn.install}
                  </button>
                )}
                <button
                  type="button"
                  className="btn"
                  disabled={installing}
                  onClick={() => {
                    void refresh();
                  }}
                >
                  {S.help.svn.checkAgain}
                </button>
              </div>
            </>
          )
        )}
      </SetupRow>
      <SetupRow title={S.help.account.title} status={counts.accounts > 0 ? "ready" : "todo"}>
        <p className="muted">
          {counts.accounts > 0 ? S.help.account.ready(counts.accounts) : S.help.account.missing}
        </p>
        <div>
          <button
            type="button"
            className="btn btn--sm"
            onClick={() => {
              onNavigate("vault");
            }}
          >
            {S.help.account.open}
          </button>
        </div>
      </SetupRow>
      <SetupRow title={S.help.ai.title} status={counts.providers > 0 ? "ready" : "optional"}>
        <p className="muted">
          {counts.providers > 0 ? S.help.ai.ready(counts.providers) : S.help.ai.missing}
        </p>
        <div>
          <button
            type="button"
            className="btn btn--sm"
            onClick={() => {
              onNavigate("providers");
            }}
          >
            {S.help.ai.open}
          </button>
        </div>
      </SetupRow>
      <SetupRow title={S.help.project.title} status={counts.projects > 0 ? "ready" : "todo"}>
        <p className="muted">
          {counts.projects > 0 ? S.help.project.ready(counts.projects) : S.help.project.missing}
        </p>
        <div>
          <button
            type="button"
            className="btn btn--sm"
            onClick={() => {
              onNavigate("projects");
            }}
          >
            {S.help.project.open}
          </button>
        </div>
      </SetupRow>
    </ol>
  );
}
