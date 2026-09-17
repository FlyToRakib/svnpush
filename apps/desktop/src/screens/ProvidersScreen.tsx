import { useEffect, useState } from "react";
import { ErrorNotice } from "../components/ErrorNotice";
import { FallbackCard } from "../components/FallbackCard";
import { Modal } from "../components/Modal";
import { ProviderForm } from "../components/ProviderForm";
import { ProviderRow } from "../components/ProviderRow";
import { ScreenHeader } from "../components/ScreenHeader";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { ProviderRecord } from "../ipc/bindings/ProviderRecord";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";
import { useProviderStore } from "../store/providerStore";
import { S } from "../strings";

type FormState = { mode: "closed" } | { mode: "add" } | { mode: "edit"; record: ProviderRecord };

interface TestResult {
  id: string;
  ok: boolean;
  message: string;
}

/** AI providers: a port of SyncDock's Providers page, Revoye first. */
export function ProvidersScreen() {
  const {
    adapters,
    file,
    loaded,
    error,
    load,
    save,
    remove,
    setDefault,
    clearAttention,
    saveFallback,
  } = useProviderStore();
  const [form, setForm] = useState<FormState>({ mode: "closed" });
  const [removing, setRemoving] = useState<ProviderRecord | null>(null);
  const [testing, setTesting] = useState<string | null>(null);
  const [result, setResult] = useState<TestResult | null>(null);
  const [actionError, setActionError] = useState<ErrorView | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  const act = async (action: () => Promise<void>) => {
    try {
      await action();
      setActionError(null);
    } catch (e) {
      setActionError(toErrorView(e));
    }
  };

  const test = async (record: ProviderRecord) => {
    setTesting(record.id);
    setResult(null);
    try {
      const message = await commands.testProvider(record.id);
      setResult({ id: record.id, ok: true, message });
      await load();
    } catch (e) {
      const view = toErrorView(e);
      setResult({
        id: record.id,
        ok: false,
        message: view.fix ? `${view.message} ${view.fix}` : view.message,
      });
    } finally {
      setTesting(null);
    }
  };

  const addButton = (
    <button
      type="button"
      className="btn btn--primary"
      onClick={() => {
        setForm({ mode: "add" });
      }}
    >
      {S.providers.add}
    </button>
  );

  return (
    <div className="providers">
      <ScreenHeader
        title={S.providers.title}
        subtitle={S.providers.subtitle}
        actions={form.mode === "closed" && addButton}
      />
      {error && <ErrorNotice error={error} />}
      {actionError && <ErrorNotice error={actionError} />}

      {form.mode !== "closed" && adapters.length > 0 && (
        <ProviderForm
          key={form.mode === "edit" ? form.record.id : "new"}
          adapters={adapters}
          editing={form.mode === "edit" ? form.record : null}
          onCancel={() => {
            setForm({ mode: "closed" });
          }}
          onSave={async (input) => {
            await save(input);
            setForm({ mode: "closed" });
          }}
        />
      )}

      {loaded && file.providers.length === 0 && form.mode === "closed" && (
        <div className="card">
          <div className="empty">
            <h2 className="empty__title">{S.providers.emptyTitle}</h2>
            <p className="empty__body">{S.providers.emptyBody}</p>
            {addButton}
          </div>
        </div>
      )}

      {file.providers.length > 0 && (
        <div className="card">
          <ul className="provider-list">
            {file.providers.map((record) => (
              <ProviderRow
                key={record.id}
                record={record}
                busy={testing !== null}
                onTest={() => {
                  void test(record);
                }}
                onDefault={() => {
                  void act(() => setDefault(record.id));
                }}
                onEdit={() => {
                  setForm({ mode: "edit", record });
                }}
                onRemove={() => {
                  setRemoving(record);
                }}
                onClear={() => {
                  void act(() => clearAttention(record.id));
                }}
              />
            ))}
          </ul>
        </div>
      )}

      {result && (
        <p className={`notice ${result.ok ? "notice--ok" : "notice--error"}`} role="status">
          {result.message}
        </p>
      )}

      {file.providers.length > 1 && (
        <FallbackCard
          key={file.providers.map((p) => p.id).join(",")}
          providers={file.providers}
          chosen={file.fallback}
          onSave={saveFallback}
        />
      )}

      <Modal
        open={removing !== null}
        title={S.providers.removeTitle}
        onClose={() => {
          setRemoving(null);
        }}
        actions={
          <>
            <button
              type="button"
              className="btn"
              onClick={() => {
                setRemoving(null);
              }}
            >
              {S.common.cancel}
            </button>
            <button
              type="button"
              className="btn btn--danger"
              onClick={() => {
                const target = removing;
                setRemoving(null);
                if (target) {
                  void act(() => remove(target.id));
                }
              }}
            >
              {S.providers.remove}
            </button>
          </>
        }
      >
        <p>{removing ? S.providers.removeBody(removing.label) : ""}</p>
      </Modal>
    </div>
  );
}
