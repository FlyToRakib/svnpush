import { useEffect, useState } from "react";
import { formatBytes } from "../format";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { FilePreview } from "../ipc/bindings/FilePreview";
import type { FileReview } from "../ipc/bindings/FileReview";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";
import { S } from "../strings";
import { ErrorNotice } from "./ErrorNotice";

interface FileReviewPanelProps {
  projectPath: string;
  review: FileReview;
  disabled: boolean;
  onConfirm: (distignore: string | null) => void;
}

/** How long typing pauses before the file list is recalculated. */
const PREVIEW_DELAY_MS = 400;
/** Left-out paths shown before "and N more". */
const LEFT_OUT_SHOWN = 40;

interface Group {
  name: string;
  count: number;
  size: number;
}

/** Files grouped by their top-level file or folder, in path order. */
function groups(preview: FilePreview): Group[] {
  const byName = new Map<string, Group>();
  for (const file of preview.files) {
    const slash = file.path.indexOf("/");
    const name = slash === -1 ? file.path : `${file.path.slice(0, slash)}/`;
    const group = byName.get(name) ?? { name, count: 0, size: 0 };
    group.count += 1;
    group.size += file.size;
    byName.set(name, group);
  }
  return [...byName.values()];
}

/** Step 5's check: what will be released, what is left out, and the rules. */
export function FileReviewPanel({
  projectPath,
  review,
  disabled,
  onConfirm,
}: FileReviewPanelProps) {
  const [text, setText] = useState(review.distignore);
  const [preview, setPreview] = useState<FilePreview>(review.preview);
  const [updating, setUpdating] = useState(false);
  const [error, setError] = useState<ErrorView | null>(null);
  const changed = text !== review.distignore;

  const edit = (next: string) => {
    setText(next);
    if (next === review.distignore) {
      setPreview(review.preview);
      setError(null);
      setUpdating(false);
    } else {
      setUpdating(true);
    }
  };

  // Recalculate the lists shortly after the rules change.
  useEffect(() => {
    if (!changed) {
      return;
    }
    let live = true;
    const timer = setTimeout(() => {
      commands.previewReleaseFiles(projectPath, text).then(
        (next) => {
          if (live) {
            setPreview(next);
            setError(null);
            setUpdating(false);
          }
        },
        (e: unknown) => {
          if (live) {
            setError(toErrorView(e));
            setUpdating(false);
          }
        },
      );
    }, PREVIEW_DELAY_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [changed, projectPath, text]);

  const size = preview.files.reduce((sum, f) => sum + f.size, 0);
  const shownLeftOut = preview.excluded.slice(0, LEFT_OUT_SHOWN);
  const hiddenLeftOut = preview.excluded_total - shownLeftOut.length;
  const saves = review.editable && (changed || !review.distignore_exists);

  return (
    <section className="stack file-review" aria-label={S.files.title}>
      <h3 className="section-title">{S.files.title}</h3>
      {review.reasons.map((reason) => (
        <p key={reason} className="notice notice--info">
          {S.files.reason[reason]}
        </p>
      ))}
      <p className="mono" aria-live="polite">
        {updating
          ? S.files.updating
          : S.files.summary(preview.files.length, formatBytes(size), preview.excluded_total)}
      </p>
      <div className="grid-2">
        <div className="stack">
          <h4 className="file-review__heading">{S.files.released}</h4>
          <ul className="file-review__list">
            {groups(preview).map((group) => (
              <li key={group.name}>
                <span className="mono">{group.name}</span>{" "}
                <span className="muted">
                  {group.name.endsWith("/") ? S.files.fileCount(group.count) : ""}{" "}
                  {formatBytes(group.size)}
                </span>
                {review.new_items.includes(group.name) && (
                  <span className="pill pill--new">{S.files.newBadge}</span>
                )}
              </li>
            ))}
          </ul>
        </div>
        <div className="stack">
          <h4 className="file-review__heading">{S.files.leftOut}</h4>
          <ul className="file-review__list">
            {shownLeftOut.map((path) => (
              <li key={path} className="mono">
                {path}
              </li>
            ))}
            {hiddenLeftOut > 0 && <li className="muted">{S.files.more(hiddenLeftOut)}</li>}
          </ul>
        </div>
      </div>
      {review.editable ? (
        <div className="field">
          <label className="field__label" htmlFor="distignore">
            {S.files.rules}
          </label>
          <textarea
            id="distignore"
            className="textarea mono"
            rows={10}
            value={text}
            disabled={disabled}
            onChange={(e) => {
              edit(e.target.value);
            }}
          />
          <p className="field__hint">{S.files.rulesHint}</p>
        </div>
      ) : (
        <p className="muted">{S.files.builtByCommand}</p>
      )}
      {error && <ErrorNotice error={error} />}
      <div>
        <button
          type="button"
          className="btn btn--primary"
          disabled={disabled || updating || error !== null}
          onClick={() => {
            onConfirm(saves ? text : null);
          }}
        >
          {saves ? S.files.save : S.files.confirm}
        </button>
      </div>
    </section>
  );
}
