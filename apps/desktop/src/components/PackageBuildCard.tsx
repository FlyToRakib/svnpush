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
  disabled: boolean;
}

/** Builds the release package without releasing, to inspect or test it. */
export function PackageBuildCard({ projectPath, disabled }: PackageBuildCardProps) {
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
        <h2 className="card__title">{S.tools.build.title}</h2>
      </div>
      <div className="card__body stack">
        <p className="muted">{S.tools.build.intro}</p>
        <div>
          <button
            type="button"
            className="btn btn--primary"
            disabled={disabled || building}
            onClick={() => {
              void build();
            }}
          >
            {building ? S.tools.build.building : S.tools.build.build}
          </button>
        </div>
        {error && <ErrorNotice error={error} />}
        {built && (
          <div className="stack" role="status">
            {built.blocked ? (
              <p className="notice notice--error">{S.tools.build.blocked}</p>
            ) : (
              <p className="notice notice--ok">{S.tools.build.ready(built.package.files.length)}</p>
            )}
            {built.blocked && <CheckTable checks={built.checks} />}
            <FileList pkg={built.package} />
          </div>
        )}
      </div>
    </section>
  );
}
