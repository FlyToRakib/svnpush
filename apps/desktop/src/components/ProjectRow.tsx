import { formatDateTime } from "../format";
import type { ProjectSummary } from "../ipc/bindings/ProjectSummary";
import { S } from "../strings";

interface ProjectRowProps {
  summary: ProjectSummary;
  onOpen: () => void;
}

/** One project in the list: name, slug, version, last release. */
export function ProjectRow({ summary, onOpen }: ProjectRowProps) {
  const { project } = summary;
  const last = project.last_release;
  const outcome = last ? S.outcome[last.outcome as keyof typeof S.outcome] : undefined;
  return (
    <tr className="project-row">
      <td>
        <button type="button" className="project-row__name" onClick={onOpen}>
          {project.name}
        </button>
        <div className="mono muted">{project.slug}</div>
        {summary.problem && <div className="project-row__problem">{summary.problem.message}</div>}
        {summary.unfinished && <span className="badge badge--warn">{S.projects.unfinished}</span>}
        {summary.locked && <span className="badge">{S.projects.locked}</span>}
      </td>
      <td className="mono">{summary.version ?? "—"}</td>
      <td>
        {last ? (
          <>
            <div>
              <span className="mono">{last.version}</span> · {outcome ?? last.outcome}
            </div>
            <div className="muted">{formatDateTime(last.finished)}</div>
          </>
        ) : (
          <span className="muted">{S.projects.neverReleased}</span>
        )}
      </td>
      <td className="num">
        <button type="button" className="btn btn--sm" onClick={onOpen}>
          {S.projects.open}
        </button>
      </td>
    </tr>
  );
}
