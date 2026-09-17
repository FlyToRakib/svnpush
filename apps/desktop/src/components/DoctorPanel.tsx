import type { DoctorReport } from "../ipc/bindings/DoctorReport";
import type { ToolReport } from "../ipc/bindings/ToolReport";
import { S } from "../strings";

interface DoctorPanelProps {
  report: DoctorReport;
}

function row(name: string, tool: ToolReport) {
  return (
    <tr key={name} className={`check check--${tool.ok ? "pass" : "fail"}`}>
      <td className="mono">{name}</td>
      <td>
        <span className={`pill pill--${tool.ok ? "pass" : "fail"}`}>
          {tool.ok ? S.settings.found : S.settings.notFound}
        </span>
      </td>
      <td>
        <div>{tool.message}</div>
        {tool.fix && (
          <div className="check__fix">
            <strong>{S.common.fix}:</strong> {tool.fix}
          </div>
        )}
      </td>
    </tr>
  );
}

/** Doctor results: svn, git and the keychain. */
export function DoctorPanel({ report }: DoctorPanelProps) {
  const keychain = report.keychain_problem;
  return (
    <table className="table">
      <tbody>
        {row("svn", report.svn)}
        {row("git", report.git)}
        <tr className={`check check--${keychain ? "fail" : "pass"}`}>
          <td>{S.settings.keychain}</td>
          <td>
            <span className={`pill pill--${keychain ? "fail" : "pass"}`}>
              {keychain ? S.settings.notFound : S.settings.found}
            </span>
          </td>
          <td>
            <div>{keychain ? keychain.message : S.settings.keychainOk}</div>
            {keychain?.fix && (
              <div className="check__fix">
                <strong>{S.common.fix}:</strong> {keychain.fix}
              </div>
            )}
          </td>
        </tr>
      </tbody>
    </table>
  );
}
