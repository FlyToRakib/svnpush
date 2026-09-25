import { useState } from "react";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { IssueLevel } from "../ipc/bindings/IssueLevel";
import type { ReadmeReport } from "../ipc/bindings/ReadmeReport";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";
import { openExternal } from "../opener";
import { S } from "../strings";
import { ErrorNotice } from "./ErrorNotice";

interface ReadmeCheckCardProps {
  projectPath: string;
  disabled: boolean;
}

const PILLS: Record<IssueLevel, string> = { Error: "fail", Warning: "warn", Note: "skip" };
const OFFICIAL_URL = "https://wordpress.org/plugins/developers/readme-validator/";

/** Validates readme.txt with WordPress.org's rules, any time, without releasing. */
export function ReadmeCheckCard({ projectPath, disabled }: ReadmeCheckCardProps) {
  const [report, setReport] = useState<ReadmeReport | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<ErrorView | null>(null);

  const check = async () => {
    setChecking(true);
    try {
      setReport(await commands.checkReadme(projectPath));
      setError(null);
    } catch (e) {
      setReport(null);
      setError(toErrorView(e));
    }
    setChecking(false);
  };

  const count = (level: IssueLevel) => report?.issues.filter((i) => i.level === level).length ?? 0;

  return (
    <section className="card">
      <div className="card__header">
        <h3 className="card__title">{S.tools.readme.title}</h3>
      </div>
      <div className="card__body stack">
        <p className="muted">{S.tools.readme.intro}</p>
        <div className="row">
          <button
            type="button"
            className="btn btn--primary"
            disabled={disabled || checking}
            onClick={() => {
              void check();
            }}
          >
            {checking ? S.tools.readme.checking : S.tools.readme.check}
          </button>
          <button
            type="button"
            className="btn"
            onClick={() => {
              void openExternal(report?.official_url ?? OFFICIAL_URL);
            }}
          >
            {S.tools.readme.official}
          </button>
        </div>
        {error && <ErrorNotice error={error} />}
        {report &&
          (report.issues.length === 0 ? (
            <p className="notice notice--ok" role="status">
              {S.tools.readme.clean}
            </p>
          ) : (
            <div className="stack" role="status">
              <p className="mono">
                {S.tools.readme.counts(count("Error"), count("Warning"), count("Note"))}
              </p>
              <ul className="readme-issues">
                {report.issues.map((issue) => (
                  <li key={issue.code}>
                    <span className={`pill pill--${PILLS[issue.level]}`}>
                      {S.tools.readme.level[issue.level]}
                    </span>{" "}
                    {issue.message}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        <p className="field__hint">{S.tools.readme.officialHint}</p>
      </div>
    </section>
  );
}
