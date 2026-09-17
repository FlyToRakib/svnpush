import { useEffect, useState, type SyntheticEvent } from "react";
import type { AdapterInfo } from "../ipc/bindings/AdapterInfo";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { Fleet } from "../ipc/bindings/Fleet";
import type { ProviderInput } from "../ipc/bindings/ProviderInput";
import type { ProviderRecord } from "../ipc/bindings/ProviderRecord";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";
import { S } from "../strings";
import { ErrorNotice } from "./ErrorNotice";

interface ProviderFormProps {
  adapters: AdapterInfo[];
  editing: ProviderRecord | null;
  onSave: (input: ProviderInput) => Promise<void>;
  onCancel: () => void;
}

/** Whether the model field is a dropdown for this adapter. */
export function usesModelPicker(adapter: AdapterInfo): boolean {
  return adapter.models.length > 0 || adapter.can_list_models;
}

function fleetLine(fleet: Fleet): string {
  return fleet.devices_online === 0
    ? S.providers.fleetNoDevice(fleet.devices_total)
    : S.providers.fleetOnline(
        fleet.devices_online,
        fleet.devices_total,
        fleet.agents_idle,
        fleet.agents_total,
        fleet.queue_depth,
      );
}

function optionLabel(model: string, fleet: Fleet | null): string {
  const kind = model.replace(/^revoye\//, "");
  const found = fleet?.providers.find((p) => p.kind === kind);
  if (!found) {
    return model;
  }
  if (found.agents_idle > 0) {
    return model + S.providers.agentsFree(found.agents_idle);
  }
  return model + (found.enabled ? S.providers.agentsBusy : S.providers.disabledKind);
}

/** Add or edit a provider. Every visibility rule comes from the adapter's metadata. */
export function ProviderForm({ adapters, editing, onSave, onCancel }: ProviderFormProps) {
  const first = adapters[0];
  const initial = adapters.find((a) => a.kind === editing?.kind) ?? first;
  const [kind, setKind] = useState(initial?.kind ?? "");
  const [label, setLabel] = useState(editing?.label ?? "");
  const [model, setModel] = useState(editing?.model ?? initial?.default_model ?? "");
  const [models, setModels] = useState<string[]>(initial?.models ?? []);
  const [modelNote, setModelNote] = useState<string | null>(null);
  const [fleet, setFleet] = useState<Fleet | null>(null);
  const [baseUrl, setBaseUrl] = useState(editing?.base_url ?? "");
  const [isDefault, setIsDefault] = useState(editing?.is_default ?? false);
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<ErrorView | null>(null);
  const adapter = adapters.find((a) => a.kind === kind);

  const loadModels = async (silent: boolean, typedKey: string) => {
    if (!adapter?.can_list_models) {
      return;
    }
    if (!typedKey.trim() && !editing?.has_key) {
      if (!silent) {
        setModelNote(S.providers.pasteKeyFirst);
      }
      return;
    }
    const target = {
      id: typedKey.trim() ? null : (editing?.id ?? null),
      kind: adapter.kind,
      base_url: baseUrl || null,
      api_key: typedKey.trim() || null,
    };
    setLoading(true);
    try {
      const list = await commands.listProviderModels(target);
      if (adapter.has_fleet) {
        setFleet(await commands.providerFleet(target).catch(() => null));
      }
      if (list.models.length > 0) {
        setModels(list.models);
        setModelNote(S.providers.modelsLoaded(list.models.length));
      } else {
        setModelNote(list.note);
      }
      setError(null);
    } catch (e) {
      if (!silent) {
        setError(toErrorView(e));
      }
    } finally {
      setLoading(false);
    }
  };

  // An existing record's key is in the keychain: fill the dropdown from the account.
  const editingId = editing?.has_key ? editing.id : null;
  const openKind = initial?.can_list_models ? initial.kind : null;
  const openFleet = initial?.has_fleet ?? false;
  const openBase = editing?.base_url ?? null;
  useEffect(() => {
    if (!editingId || !openKind) {
      return;
    }
    let live = true;
    const target = { id: editingId, kind: openKind, base_url: openBase, api_key: null };
    commands.listProviderModels(target).then(
      (list) => {
        if (live && list.models.length > 0) {
          setModels(list.models);
          setModelNote(S.providers.modelsLoaded(list.models.length));
        }
      },
      () => undefined,
    );
    if (openFleet) {
      commands.providerFleet(target).then(
        (snapshot) => {
          if (live) {
            setFleet(snapshot);
          }
        },
        () => undefined,
      );
    }
    return () => {
      live = false;
    };
  }, [editingId, openKind, openFleet, openBase]);

  const chooseKind = (next: string) => {
    const nextAdapter = adapters.find((a) => a.kind === next);
    setKind(next);
    setModel(nextAdapter?.default_model ?? "");
    setModels(nextAdapter?.models ?? []);
    setModelNote(null);
    setFleet(null);
    setBaseUrl("");
    setError(null);
  };

  const submit = async (event: SyntheticEvent) => {
    event.preventDefault();
    if (!adapter) {
      return;
    }
    try {
      await onSave({
        id: editing?.id ?? null,
        kind,
        label,
        model,
        base_url: adapter.fixed_base_url ? null : baseUrl || null,
        is_default: isDefault,
        api_key: adapter.keyless ? null : apiKey || null,
      });
    } catch (e) {
      setError(toErrorView(e));
    }
  };

  if (!adapter) {
    return null;
  }
  const picker = usesModelPicker(adapter);
  const modelOptions = models.includes(model) || !model ? models : [model, ...models];

  return (
    <section className="card">
      <div className="card__header">
        <h2 className="card__title">{editing ? S.providers.formEdit : S.providers.formAdd}</h2>
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
        <div className="grid-2">
          <div className="field">
            <label className="field__label" htmlFor="pf-kind">
              {S.providers.provider}
            </label>
            <select
              id="pf-kind"
              className="select"
              value={kind}
              onChange={(e) => {
                chooseKind(e.target.value);
              }}
            >
              {adapters.map((a) => (
                <option key={a.kind} value={a.kind}>
                  {a.label}
                  {a.recommended ? S.providers.recommended : ""}
                </option>
              ))}
            </select>
            <p className="field__hint">{adapter.note}</p>
          </div>
          <div className="field">
            <label className="field__label" htmlFor="pf-label">
              {S.providers.label}
            </label>
            <input
              id="pf-label"
              className="input"
              value={label}
              placeholder={S.providers.labelPlaceholder}
              onChange={(e) => {
                setLabel(e.target.value);
              }}
            />
          </div>
          <div className="field">
            <label className="field__label" htmlFor="pf-model">
              {S.providers.model}
            </label>
            {picker ? (
              <div className="input-group">
                <select
                  id="pf-model"
                  className="select mono"
                  value={model}
                  onChange={(e) => {
                    setModel(e.target.value);
                  }}
                >
                  {modelOptions.map((m) => (
                    <option key={m} value={m}>
                      {optionLabel(m, fleet)}
                    </option>
                  ))}
                </select>
                {adapter.can_list_models && (
                  <button
                    type="button"
                    className="btn"
                    aria-label={S.providers.loadModels}
                    title={S.providers.loadModels}
                    disabled={loading}
                    onClick={() => {
                      void loadModels(false, apiKey);
                    }}
                  >
                    ↻
                  </button>
                )}
              </div>
            ) : (
              <input
                id="pf-model"
                className="input mono"
                value={model}
                onChange={(e) => {
                  setModel(e.target.value);
                }}
              />
            )}
            <p className="field__hint">
              {loading ? S.providers.loadingModels : (modelNote ?? adapter.model_hint)}
            </p>
            {fleet && <p className="field__hint">{fleetLine(fleet)}</p>}
          </div>
          {!adapter.keyless && (
            <div className="field">
              <label className="field__label" htmlFor="pf-key">
                {S.providers.apiKey}
              </label>
              <div className="input-group">
                <input
                  id="pf-key"
                  className="input mono"
                  type={showKey ? "text" : "password"}
                  autoComplete="off"
                  value={apiKey}
                  placeholder={
                    editing?.has_key ? S.providers.keyUnchanged : adapter.key_placeholder
                  }
                  onChange={(e) => {
                    setApiKey(e.target.value);
                  }}
                  onBlur={(e) => {
                    if (e.target.value.trim()) {
                      void loadModels(true, e.target.value);
                    }
                  }}
                />
                <button
                  type="button"
                  className="btn"
                  aria-pressed={showKey}
                  aria-label={showKey ? S.providers.hideKey : S.providers.showKey}
                  onClick={() => {
                    setShowKey(!showKey);
                  }}
                >
                  {showKey ? S.providers.hideKey : S.providers.showKey}
                </button>
              </div>
              <p className="field__hint">{S.providers.keyHint}</p>
            </div>
          )}
        </div>

        <details
          className="advanced"
          open={adapter.kind === "openai_compatible" || adapter.kind === "local"}
        >
          <summary>{S.providers.advanced}</summary>
          <div className="stack advanced__body">
            {!adapter.fixed_base_url && (
              <div className="field">
                <label className="field__label" htmlFor="pf-base">
                  {S.providers.baseUrl}
                </label>
                <input
                  id="pf-base"
                  className="input mono"
                  type="url"
                  value={baseUrl}
                  placeholder={adapter.default_base_url || "https://…"}
                  onChange={(e) => {
                    setBaseUrl(e.target.value);
                  }}
                />
                <p className="field__hint">{S.providers.baseUrlHint}</p>
              </div>
            )}
            <label className="checkbox">
              <input
                type="checkbox"
                checked={isDefault}
                onChange={(e) => {
                  setIsDefault(e.target.checked);
                }}
              />
              {S.providers.defaultToggle}
            </label>
          </div>
        </details>

        {error && <ErrorNotice error={error} />}
        <div>
          <button type="submit" className="btn btn--primary">
            {S.providers.save}
          </button>
        </div>
      </form>
    </section>
  );
}
