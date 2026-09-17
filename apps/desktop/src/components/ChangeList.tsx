import type { ChangeSet } from "../ipc/bindings/ChangeSet";
import { S } from "../strings";

interface ChangeListProps {
  changes: ChangeSet;
}

function sourceText(changes: ChangeSet): string {
  switch (changes.source) {
    case "Git":
      return S.changes.sourceGit(changes.base ?? "");
    case "Trunk":
      return S.changes.sourceTrunk;
    case "FirstRelease":
      return S.changes.sourceFirst;
    case "Unknown":
      return S.changes.sourceUnknown;
  }
}

/** Step 2: the files and commits since the previous release. */
export function ChangeList({ changes }: ChangeListProps) {
  return (
    <section className="stack" aria-label={S.changes.title}>
      <h3 className="section-title">{S.changes.title}</h3>
      <p className="muted">{sourceText(changes)}</p>
      {changes.files.length === 0 ? (
        <p className="muted">{S.changes.none}</p>
      ) : (
        <ul className="file-changes">
          {changes.files.map((file) => (
            <li key={file.path} className={`file-change file-change--${file.kind.toLowerCase()}`}>
              <span className="file-change__kind">{S.changes.kind[file.kind]}</span>
              <span className="mono">{file.path}</span>
            </li>
          ))}
        </ul>
      )}
      {changes.commits.length > 0 && (
        <>
          <h4 className="subsection-title">{S.changes.commits}</h4>
          <ul className="commits">
            {changes.commits.map((subject, index) => (
              <li key={`${String(index)}-${subject}`}>{subject}</li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
