import { formatDateTime, runIdToIso } from "../format";
import type { RunJournal } from "../ipc/bindings/RunJournal";
import { S } from "../strings";

interface PastReleasesProps {
  journals: RunJournal[];
}

/** A small table of past runs, newest first. */
export function PastReleases({ journals }: PastReleasesProps) {
  return (
    <section className="card">
      <div className="card__header">
        <h2 className="card__title">{S.history.title}</h2>
      </div>
      <div className="card__body">
        {journals.length === 0 ? (
          <p className="muted">{S.history.empty}</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th scope="col">{S.history.date}</th>
                <th scope="col">{S.history.version}</th>
                <th scope="col">{S.history.result}</th>
                <th scope="col">{S.history.revisions}</th>
              </tr>
            </thead>
            <tbody>
              {journals.map((j) => (
                <tr key={j.id}>
                  <td>{formatDateTime(runIdToIso(j.id))}</td>
                  <td className="mono">{j.version ?? "—"}</td>
                  <td>
                    {j.outcome ? S.outcome[j.outcome.kind] : "—"}
                    {j.dry_run && j.outcome?.kind !== "DryRun" ? ` (${S.history.dryRun})` : ""}
                  </td>
                  <td className="mono">
                    {[j.revisions.trunk, j.revisions.tag]
                      .filter((r): r is number => r !== null)
                      .map((r) => `r${String(r)}`)
                      .join(" · ") || "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}
