import { useEffect } from "react";
import type { AiChoice } from "../ipc/bindings/AiChoice";
import { useProviderStore } from "../store/providerStore";
import { S } from "../strings";

interface AiSettingsFieldsProps {
  choice: AiChoice;
  patterns: string;
  onChoice: (choice: AiChoice) => void;
  onPatterns: (patterns: string) => void;
}

const encode = (choice: AiChoice) =>
  choice.mode === "Pinned" ? `pinned:${choice.provider_id}` : choice.mode;

const decode = (value: string): AiChoice => {
  if (value.startsWith("pinned:")) {
    return { mode: "Pinned", provider_id: value.slice("pinned:".length) };
  }
  return value === "Off" ? { mode: "Off" } : { mode: "Default" };
};

/** The project's AI provider (default, pinned or off) and exclude patterns (plan §6.2). */
export function AiSettingsFields({
  choice,
  patterns,
  onChoice,
  onPatterns,
}: AiSettingsFieldsProps) {
  const { file, loaded, load } = useProviderStore();

  useEffect(() => {
    if (!loaded) {
      void load();
    }
  }, [loaded, load]);

  const providers = file.providers;
  const pinnedMissing =
    choice.mode === "Pinned" && !providers.some((p) => p.id === choice.provider_id);

  return (
    <div className="grid-2">
      <div className="field">
        <label className="field__label" htmlFor="ps-ai">
          {S.projectSettings.aiProvider}
        </label>
        <select
          id="ps-ai"
          className="input"
          value={encode(choice)}
          onChange={(e) => {
            onChoice(decode(e.target.value));
          }}
        >
          <option value="Default">{S.projectSettings.aiDefault}</option>
          {providers.map((provider) => (
            <option key={provider.id} value={`pinned:${provider.id}`}>
              {S.projectSettings.aiPinned(provider.label)}
            </option>
          ))}
          {pinnedMissing && <option value={encode(choice)}>{encode(choice)}</option>}
          <option value="Off">{S.projectSettings.aiOff}</option>
        </select>
        <p className="field__hint">{S.projectSettings.aiProviderHint}</p>
      </div>
      <div className="field">
        <label className="field__label" htmlFor="ps-ai-exclude">
          {S.projectSettings.aiExclude}
        </label>
        <textarea
          id="ps-ai-exclude"
          className="textarea textarea--short mono"
          value={patterns}
          onChange={(e) => {
            onPatterns(e.target.value);
          }}
        />
        <p className="field__hint">{S.projectSettings.aiExcludeHint}</p>
      </div>
    </div>
  );
}
