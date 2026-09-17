import { useState, type SyntheticEvent } from "react";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { Project } from "../ipc/bindings/Project";
import type { ProjectSettings } from "../ipc/bindings/ProjectSettings";
import { toErrorView } from "../ipc/tauri";
import { S } from "../strings";
import { AiSettingsFields } from "./AiSettingsFields";
import { ErrorNotice } from "./ErrorNotice";

interface ProjectSettingsFormProps {
  project: Project;
  disabled: boolean;
  onSave: (svnUrl: string, settings: ProjectSettings) => Promise<void>;
  onRemove: () => void;
}

const lines = (text: string) =>
  text
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);

const orNull = (text: string) => (text.trim() ? text.trim() : null);

/** Project settings, all optional (plan §6.2). */
export function ProjectSettingsForm({
  project,
  disabled,
  onSave,
  onRemove,
}: ProjectSettingsFormProps) {
  const [svnUrl, setSvnUrl] = useState(project.svn_url);
  const [settings, setSettings] = useState<ProjectSettings>(project.settings);
  const [required, setRequired] = useState(project.settings.required_paths.join("\n"));
  const [exclude, setExclude] = useState(project.settings.ai_exclude_patterns.join("\n"));
  const [error, setError] = useState<ErrorView | null>(null);
  const [saved, setSaved] = useState(false);

  const change = (next: Partial<ProjectSettings>) => {
    setSettings({ ...settings, ...next });
    setSaved(false);
  };

  const submit = async (event: SyntheticEvent) => {
    event.preventDefault();
    try {
      await onSave(svnUrl, {
        ...settings,
        required_paths: lines(required),
        ai_exclude_patterns: lines(exclude),
      });
      setError(null);
      setSaved(true);
    } catch (e) {
      setError(toErrorView(e));
    }
  };

  const noAssets = settings.assets_folder === "";

  return (
    <details className="card settings">
      <summary className="card__header card__summary">
        <h2 className="card__title">{S.projectSettings.title}</h2>
      </summary>
      <form
        className="card__body stack"
        onSubmit={(e) => {
          void submit(e);
        }}
      >
        <div className="field">
          <label className="field__label" htmlFor="ps-svn">
            {S.projectSettings.svnUrl}
          </label>
          <input
            id="ps-svn"
            className="input mono"
            value={svnUrl}
            onChange={(e) => {
              setSvnUrl(e.target.value);
            }}
          />
        </div>
        <div className="grid-2">
          <div className="field">
            <label className="field__label" htmlFor="ps-root">
              {S.projectSettings.packageRoot}
            </label>
            <input
              id="ps-root"
              className="input mono"
              value={settings.package_root}
              onChange={(e) => {
                change({ package_root: e.target.value });
              }}
            />
            <p className="field__hint">{S.projectSettings.packageRootHint}</p>
          </div>
          <div className="field">
            <label className="field__label" htmlFor="ps-main">
              {S.projectSettings.mainFile}
            </label>
            <input
              id="ps-main"
              className="input mono"
              value={settings.main_file ?? ""}
              onChange={(e) => {
                change({ main_file: orNull(e.target.value) });
              }}
            />
            <p className="field__hint">{S.projectSettings.mainFileHint}</p>
          </div>
        </div>

        <fieldset className="fieldset">
          <legend className="field__label">{S.projectSettings.versionLocations}</legend>
          <p className="field__hint">{S.projectSettings.versionLocationsHint}</p>
          {settings.version_locations.map((location, index) => (
            <div key={index} className="location">
              <input
                className="input mono"
                aria-label={`${S.projectSettings.locationPath} ${String(index + 1)}`}
                placeholder={S.projectSettings.locationPath}
                value={location.path}
                onChange={(e) => {
                  const next = [...settings.version_locations];
                  next[index] = { ...location, path: e.target.value };
                  change({ version_locations: next });
                }}
              />
              <input
                className="input mono"
                aria-label={`${S.projectSettings.locationPattern} ${String(index + 1)}`}
                placeholder={S.projectSettings.locationPattern}
                value={location.pattern}
                onChange={(e) => {
                  const next = [...settings.version_locations];
                  next[index] = { ...location, pattern: e.target.value };
                  change({ version_locations: next });
                }}
              />
              <button
                type="button"
                className="btn btn--sm btn--danger"
                onClick={() => {
                  change({
                    version_locations: settings.version_locations.filter((_, i) => i !== index),
                  });
                }}
              >
                {S.common.remove}
              </button>
            </div>
          ))}
          <div>
            <button
              type="button"
              className="btn btn--sm"
              onClick={() => {
                change({
                  version_locations: [...settings.version_locations, { path: "", pattern: "" }],
                });
              }}
            >
              {S.projectSettings.addLocation}
            </button>
          </div>
        </fieldset>

        <div className="field">
          <label className="field__label" htmlFor="ps-required">
            {S.projectSettings.requiredPaths}
          </label>
          <textarea
            id="ps-required"
            className="textarea textarea--short"
            value={required}
            onChange={(e) => {
              setRequired(e.target.value);
              setSaved(false);
            }}
          />
          <p className="field__hint">{S.projectSettings.requiredPathsHint}</p>
        </div>
        <div className="field">
          <label className="field__label" htmlFor="ps-hook">
            {S.projectSettings.preBuild}
          </label>
          <input
            id="ps-hook"
            className="input mono"
            value={settings.pre_build_command ?? ""}
            onChange={(e) => {
              change({ pre_build_command: orNull(e.target.value) });
            }}
          />
          <p className="field__hint">{S.projectSettings.preBuildHint}</p>
        </div>
        <div className="grid-2">
          <div className="field">
            <label className="field__label" htmlFor="ps-assets">
              {S.projectSettings.assetsFolder}
            </label>
            <input
              id="ps-assets"
              className="input mono"
              value={settings.assets_folder ?? ""}
              disabled={noAssets}
              onChange={(e) => {
                change({ assets_folder: orNull(e.target.value) });
              }}
            />
            <p className="field__hint">{S.projectSettings.assetsFolderHint}</p>
            <label className="checkbox">
              <input
                type="checkbox"
                checked={noAssets}
                onChange={(e) => {
                  change({ assets_folder: e.target.checked ? "" : null });
                }}
              />
              {S.projectSettings.noAssets}
            </label>
          </div>
          <div className="field">
            <label className="field__label" htmlFor="ps-account">
              {S.projectSettings.svnAccount}
            </label>
            <input
              id="ps-account"
              className="input mono"
              value={settings.svn_account ?? ""}
              onChange={(e) => {
                change({ svn_account: orNull(e.target.value) });
              }}
            />
            <p className="field__hint">{S.projectSettings.svnAccountHint}</p>
          </div>
        </div>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={settings.allow_phar}
            onChange={(e) => {
              change({ allow_phar: e.target.checked });
            }}
          />
          {S.projectSettings.allowPhar}
        </label>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={settings.post_publish_git_tag}
            onChange={(e) => {
              change({ post_publish_git_tag: e.target.checked });
            }}
          />
          {S.projectSettings.gitTag}
        </label>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={settings.post_publish_open_page}
            onChange={(e) => {
              change({ post_publish_open_page: e.target.checked });
            }}
          />
          {S.projectSettings.openPage}
        </label>

        <AiSettingsFields
          choice={settings.ai_provider}
          patterns={exclude}
          onChoice={(choice) => {
            change({ ai_provider: choice });
          }}
          onPatterns={(text) => {
            setExclude(text);
            setSaved(false);
          }}
        />

        {error && <ErrorNotice error={error} />}
        {saved && <p className="notice notice--ok">{S.projectSettings.saved}</p>}
        <div className="row row--between">
          <button type="submit" className="btn btn--primary" disabled={disabled}>
            {S.common.save}
          </button>
          <button type="button" className="btn btn--danger" disabled={disabled} onClick={onRemove}>
            {S.projectSettings.removeProject}
          </button>
        </div>
      </form>
    </details>
  );
}
