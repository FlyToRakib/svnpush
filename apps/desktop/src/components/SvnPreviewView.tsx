import { useState } from "react";
import type { Delta } from "../ipc/bindings/Delta";
import type { SvnPreview } from "../ipc/bindings/SvnPreview";
import { S } from "../strings";
import { DiffView } from "./DiffView";

interface SvnPreviewViewProps {
  preview: SvnPreview;
}

/** Step 6: Added, Modified and Deleted for trunk and assets, with a diff per file. */
export function SvnPreviewView({ preview }: SvnPreviewViewProps) {
  const [open, setOpen] = useState<string | null>(null);
  const diffs = new Map(preview.diffs.map((d) => [d.path, d.diff]));

  const group = (area: string, label: string, paths: string[]) =>
    paths.length > 0 && (
      <div className="delta__group">
        <h5 className="delta__label">
          {label} ({paths.length})
        </h5>
        <ul className="delta__list">
          {paths.map((path) => {
            const file = `${area}/${path}`;
            const diff = diffs.get(file);
            return (
              <li key={file}>
                {diff === undefined ? (
                  <span className="mono">{path}</span>
                ) : (
                  <button
                    type="button"
                    className="btn--link mono"
                    aria-expanded={open === file}
                    onClick={() => {
                      setOpen(open === file ? null : file);
                    }}
                  >
                    {path}
                  </button>
                )}
                {open === file && diff !== undefined && (
                  <DiffView diff={diff} label={S.preview.diffTitle(file)} />
                )}
              </li>
            );
          })}
        </ul>
      </div>
    );

  const area = (name: "trunk" | "assets", title: string, delta: Delta) => (
    <section className="delta" aria-label={title}>
      <h4 className="subsection-title mono">{title}</h4>
      {delta.added.length + delta.modified.length + delta.deleted.length === 0 ? (
        <p className="muted">{S.preview.nothing}</p>
      ) : (
        <>
          {group(name, S.preview.added, delta.added)}
          {group(name, S.preview.modified, delta.modified)}
          {group(name, S.preview.deleted, delta.deleted)}
        </>
      )}
    </section>
  );

  return (
    <div className="stack">
      {area("trunk", S.preview.trunk, preview.trunk)}
      {area("assets", S.preview.assets, preview.assets)}
      <dl className="facts">
        <dt>{S.preview.tagUrl}</dt>
        <dd className="mono break">{preview.tag_url}</dd>
      </dl>
    </div>
  );
}
