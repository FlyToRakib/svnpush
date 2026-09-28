import { open } from "@tauri-apps/plugin-dialog";
import { useState, type SyntheticEvent } from "react";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { FolderInspection } from "../ipc/bindings/FolderInspection";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";
import { S } from "../strings";
import { ErrorNotice } from "./ErrorNotice";
import { useFormFocus } from "./useFormFocus";

interface AddProjectFormProps {
  onAdd: (folder: string, svnUrl: string, mainFile: string | null) => Promise<void>;
  onCancel: () => void;
  /** The id of the element to focus when the form closes, if the button that opened it is gone. */
  returnFocus?: string;
}

/** Folder, then detection, then the SVN URL (plan §6.1). */
export function AddProjectForm({ onAdd, onCancel, returnFocus }: AddProjectFormProps) {
  const title = useFormFocus<HTMLHeadingElement>(returnFocus);
  const [inspection, setInspection] = useState<FolderInspection | null>(null);
  const [svnUrl, setSvnUrl] = useState("");
  const [mainFile, setMainFile] = useState("");
  const [error, setError] = useState<ErrorView | null>(null);
  const [saving, setSaving] = useState(false);

  const choose = async () => {
    try {
      const folder = await open({ directory: true, multiple: false });
      if (typeof folder !== "string") {
        return;
      }
      const found = await commands.inspectFolder(folder);
      setInspection(found);
      setSvnUrl(found.suggested_svn_url ?? "");
      setMainFile(
        found.main_file_candidates.length > 1 ? (found.main_file_candidates[0] ?? "") : "",
      );
      setError(null);
    } catch (e) {
      setError(toErrorView(e));
    }
  };

  const submit = async (event: SyntheticEvent) => {
    event.preventDefault();
    if (!inspection) {
      return;
    }
    setSaving(true);
    try {
      await onAdd(inspection.folder, svnUrl, mainFile || null);
    } catch (e) {
      setError(toErrorView(e));
    } finally {
      setSaving(false);
    }
  };

  const candidates = inspection?.main_file_candidates ?? [];

  return (
    <section className="card">
      <div className="card__header">
        <h2 ref={title} className="card__title" tabIndex={-1}>
          {S.projects.addTitle}
        </h2>
        <button type="button" className="btn btn--ghost btn--sm" onClick={onCancel}>
          {S.common.cancel}
        </button>
      </div>
      <form
        className="card__body stack"
        onSubmit={(e) => {
          void submit(e);
        }}
      >
        <div className="field">
          <label className="field__label" htmlFor="add-folder">
            {S.projects.folder}
          </label>
          <div className="input-group">
            <output
              id="add-folder"
              className="input mono input--readonly"
              title={inspection?.folder}
            >
              {inspection?.folder ?? ""}
            </output>
            <button
              type="button"
              className="btn"
              onClick={() => {
                void choose();
              }}
            >
              {S.projects.chooseFolder}
            </button>
          </div>
          <p className="field__hint">
            {inspection?.name && inspection.version
              ? S.projects.detected(inspection.name, inspection.version)
              : inspection && candidates.length === 0
                ? S.projects.noMainFile
                : S.projects.folderHint}
          </p>
        </div>
        {candidates.length > 1 && (
          <div className="field">
            <label className="field__label" htmlFor="add-main">
              {S.projects.mainFile}
            </label>
            <select
              id="add-main"
              className="select mono"
              value={mainFile}
              onChange={(e) => {
                setMainFile(e.target.value);
              }}
            >
              {candidates.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
            <p className="field__hint">{S.projects.mainFileHint}</p>
          </div>
        )}
        <div className="field">
          <label className="field__label" htmlFor="add-svn">
            {S.projects.svnUrl}
          </label>
          <input
            id="add-svn"
            className="input mono"
            value={svnUrl}
            placeholder="https://plugins.svn.wordpress.org/my-plugin"
            onChange={(e) => {
              setSvnUrl(e.target.value);
            }}
          />
          <p className="field__hint">{S.projects.svnUrlHint}</p>
        </div>
        {error && <ErrorNotice error={error} />}
        <div>
          <button
            type="submit"
            className="btn btn--primary"
            disabled={!inspection || candidates.length === 0 || !svnUrl.trim() || saving}
          >
            {S.common.save}
          </button>
        </div>
      </form>
    </section>
  );
}
