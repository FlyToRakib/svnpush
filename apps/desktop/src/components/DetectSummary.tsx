import type { RunState } from "../ipc/bindings/RunState";
import { S } from "../strings";

interface DetectSummaryProps {
  state: RunState;
}

/** Step 1: what was detected. */
export function DetectSummary({ state }: DetectSummaryProps) {
  const facts = state.facts;
  if (!facts) {
    return null;
  }
  const git = state.git;
  return (
    <div className="stack">
      <dl className="facts">
        <dt>{S.detect.mainFile}</dt>
        <dd className="mono">{facts.main_file}</dd>
        <dt>{S.detect.slug}</dt>
        <dd className="mono">{facts.slug}</dd>
        <dt>{S.detect.textDomain}</dt>
        <dd className="mono">{facts.header.text_domain ?? S.detect.missing}</dd>
        <dt>{S.detect.previous}</dt>
        <dd className="mono">{state.previous ?? S.detect.firstRelease}</dd>
        <dt>{S.detect.tags}</dt>
        <dd className="mono">
          {state.server_tags.length > 0 ? state.server_tags.join(", ") : S.common.none}
        </dd>
        <dt>{S.detect.git}</dt>
        <dd>
          {git
            ? [git.branch ? S.detect.branch(git.branch) : null, S.detect.dirty(git.dirty.length)]
                .filter(Boolean)
                .join(" · ")
            : S.detect.noGit}
        </dd>
      </dl>
      <table className="table">
        <caption className="table__caption">{S.detect.versions}</caption>
        <tbody>
          {facts.versions.map((source) => (
            <tr key={`${source.label}-${source.path}-${String(source.line)}`}>
              <td>{source.label}</td>
              <td className="mono">
                {source.path}
                {source.line !== null ? `:${String(source.line)}` : ""}
              </td>
              <td className="mono">{source.value || S.detect.missing}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
