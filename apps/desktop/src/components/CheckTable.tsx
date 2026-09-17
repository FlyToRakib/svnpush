import type { CheckResult } from "../ipc/bindings/CheckResult";
import { S } from "../strings";

interface CheckTableProps {
  checks: CheckResult[];
}

function rows(checks: CheckResult[]) {
  return (
    <>
      {checks.map((check) => (
        <tr key={check.id} className={`check check--${check.status.toLowerCase()}`}>
          <td className="mono check__id">{check.id}</td>
          <td>
            <span className={`pill pill--${check.status.toLowerCase()}`}>
              {S.checks.status[check.status]}
            </span>
          </td>
          <td>
            <div className="check__title">{check.title}</div>
            <div className="check__message">{check.message}</div>
            {check.status === "Fail" && check.fix && (
              <div className="check__fix">
                <strong>{S.common.fix}:</strong> {check.fix}
              </div>
            )}
            {check.status === "Fail" && check.paths.length > 0 && (
              <ul className="check__paths">
                {check.paths.map((path) => (
                  <li key={path} className="mono">
                    {path}
                  </li>
                ))}
              </ul>
            )}
          </td>
        </tr>
      ))}
    </>
  );
}

/** Step 4: the blocking checks and warnings, with fixes for failures. */
export function CheckTable({ checks }: CheckTableProps) {
  if (checks.length === 0) {
    return <p className="muted">{S.checks.none}</p>;
  }
  const blocking = checks.filter((c) => c.severity === "Block");
  const warnings = checks.filter((c) => c.severity === "Warn");
  return (
    <div className="stack">
      <table className="table checks">
        <caption className="table__caption">{S.checks.blocking}</caption>
        <tbody>{rows(blocking)}</tbody>
      </table>
      <table className="table checks">
        <caption className="table__caption">{S.checks.warnings}</caption>
        <tbody>{rows(warnings)}</tbody>
      </table>
    </div>
  );
}
