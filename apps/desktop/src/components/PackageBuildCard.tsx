import { useState } from "react";
import type { BuiltPackage } from "../ipc/bindings/BuiltPackage";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";
import { S } from "../strings";
import { CheckTable } from "./CheckTable";
import { ErrorNotice } from "./ErrorNotice";
import { FileList } from "./FileList";

interface PackageBuildCardProps {
  projectPath: string;
  /** The version in the plugin files now, which is the version this builds. */
  currentVersion: string | null;
  disabled: boolean;
}

/** Builds the release package without releasing, to inspect or test it. */
export function PackageBuildCard({ projectPath, currentVersion, disabled }: PackageBuildCardProps) {
  const [built, setBuilt] = useState<BuiltPackage | null>(null);
  const [building, setBuilding] = useState(false);
  const [error, setError] = useState<ErrorView | null>(null);

  const build = async () => {
    setBuilding(true);
    try {
      setBuilt(await commands.buildPackage(projectPath));
      setError(null);
    } catch (e) {
      setBuilt(null);
      setError(toErrorView(e));
    }
    setBuilding(false);
  };

  return (
    <section className="card">
      <div className="card__header">
        <h3 className="card__title">{S.tools.build.title}</h3>
      </div>
      <div className="card__body stack">
        <p className="muted">{S.tools.build.intro}</p>
        {currentVersion && <p>{S.tools.build.current(currentVersion)}</p>}
        <p className="muted">{S.tools.build.newVersionNote}</p>
        <div>
          <button
            type="button"
            className="btn btn--primary"
            disabled={disabled || building}
            onClick={() => {
              void build();
            }}
          >
            {building
              ? S.tools.build.building
              : built
                ? S.tools.build.rebuild
                : S.tools.build.build}
          </button>
        </div>
        {error && <ErrorNotice error={error} />}
        {built && (
          <div className="stack" role="status">
            {built.blocked ? (
              <p className="notice notice--error">{S.tools.build.blocked}</p>
            ) : (
              <p className="notice notice--ok">
                {S.tools.build.builtVersion(built.version)}.{" "}
                {S.tools.build.ready(built.package.files.length)}
              </p>
            )}
            {built.blocked && <CheckTable checks={built.checks} />}
            <FileList pkg={built.package} />
          </div>
        )}
      </div>
    </section>
  );
}
