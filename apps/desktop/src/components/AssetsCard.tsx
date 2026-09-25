import { useEffect, useState } from "react";
import { formatBytes } from "../format";
import type { AssetReport } from "../ipc/bindings/AssetReport";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";
import { openPath } from "../opener";
import { S } from "../strings";
import { AssetGuide } from "./AssetGuide";
import { ErrorNotice } from "./ErrorNotice";

interface AssetsCardProps {
  projectPath: string;
  disabled: boolean;
}

/** The icon, banner and screenshots: where they live and whether WordPress.org will use them. */
export function AssetsCard({ projectPath, disabled }: AssetsCardProps) {
  const [report, setReport] = useState<AssetReport | null>(null);
  const [error, setError] = useState<ErrorView | null>(null);

  const load = async (action: (path: string) => Promise<AssetReport>) => {
    try {
      setReport(await action(projectPath));
      setError(null);
    } catch (e) {
      setError(toErrorView(e));
    }
  };

  useEffect(() => {
    let live = true;
    commands.checkAssets(projectPath).then(
      (next) => {
        if (live) setReport(next);
      },
      (e: unknown) => {
        if (live) setError(toErrorView(e));
      },
    );
    return () => {
      live = false;
    };
  }, [projectPath]);

  const folder = report?.folder ?? null;

  return (
    <section className="card">
      <div className="card__header">
        <h3 className="card__title">{S.tools.assets.title}</h3>
      </div>
      <div className="card__body stack">
        <p className="muted">{S.tools.assets.intro}</p>
        {folder && <p className="mono break">{folder}</p>}
        {error && <ErrorNotice error={error} />}
        {report && !report.exists && folder && (
          <div className="row">
            <span>{S.tools.assets.none}</span>
            <button
              type="button"
              className="btn btn--primary"
              disabled={disabled}
              onClick={() => {
                void load(commands.createAssetsFolder);
              }}
            >
              {S.tools.assets.create}
            </button>
          </div>
        )}
        {report?.exists && (
          <>
            {report.files.length === 0 ? (
              <p className="muted">{S.tools.assets.empty}</p>
            ) : (
              <table className="table">
                <thead>
                  <tr>
                    <th scope="col">{S.tools.assets.columns.file}</th>
                    <th scope="col">{S.tools.assets.columns.role}</th>
                    <th scope="col">{S.tools.assets.columns.pixels}</th>
                    <th scope="col">{S.tools.assets.columns.status}</th>
                  </tr>
                </thead>
                <tbody>
                  {report.files.map((file) => (
                    <tr key={file.name}>
                      <td className="mono">{file.name}</td>
                      <td>{S.tools.assets.role[file.kind]}</td>
                      <td className="mono">
                        {file.width !== null && file.height !== null
                          ? `${String(file.width)} × ${String(file.height)}`
                          : "—"}{" "}
                        <span className="muted">
                          {file.bytes > 0 ? formatBytes(file.bytes) : ""}
                        </span>
                      </td>
                      <td>
                        {file.problem ? (
                          <span className="pill pill--warn">{file.problem}</span>
                        ) : (
                          <span className="pill pill--pass">{S.tools.assets.ok}</span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            {report.suggestions.length > 0 && (
              <ul className="notice notice--info notices">
                {report.suggestions.map((s) => (
                  <li key={s}>{s}</li>
                ))}
              </ul>
            )}
            <div className="row">
              <button
                type="button"
                className="btn btn--sm"
                onClick={() => {
                  void load(commands.checkAssets);
                }}
              >
                {S.tools.assets.checkAgain}
              </button>
              {folder && (
                <button
                  type="button"
                  className="btn btn--sm"
                  onClick={() => {
                    void openPath(folder);
                  }}
                >
                  {S.tools.assets.open}
                </button>
              )}
            </div>
          </>
        )}
        <details>
          <summary>{S.tools.assets.guideTitle}</summary>
          <AssetGuide />
        </details>
      </div>
    </section>
  );
}
