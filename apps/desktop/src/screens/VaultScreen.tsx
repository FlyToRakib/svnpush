import { useEffect, useState } from "react";
import { AccountForm } from "../components/AccountForm";
import { ErrorNotice } from "../components/ErrorNotice";
import { Modal } from "../components/Modal";
import { ScreenHeader } from "../components/ScreenHeader";
import type { AccountView } from "../ipc/bindings/AccountView";
import type { ErrorView } from "../ipc/bindings/ErrorView";
import type { VaultView } from "../ipc/bindings/VaultView";
import { commands } from "../ipc/commands";
import { toErrorView } from "../ipc/tauri";
import { S } from "../strings";

type FormState = { mode: "closed" } | { mode: "add" } | { mode: "edit"; account: AccountView };

const key = (a: AccountView) => `${a.host}/${a.username}`;

/** SVN accounts. Passwords go to the keychain and never come back to the UI. */
export function VaultScreen() {
  const [view, setView] = useState<VaultView | null>(null);
  const [form, setForm] = useState<FormState>({ mode: "closed" });
  const [error, setError] = useState<ErrorView | null>(null);
  const [notice, setNotice] = useState<{ ok: boolean; text: string } | null>(null);
  const [testing, setTesting] = useState<string | null>(null);
  const [removing, setRemoving] = useState<AccountView | null>(null);

  useEffect(() => {
    commands.vaultView().then(setView, (e: unknown) => {
      setError(toErrorView(e));
    });
  }, []);

  const run = async (action: () => Promise<VaultView>) => {
    try {
      setView(await action());
      setError(null);
    } catch (e) {
      setError(toErrorView(e));
      throw e;
    }
  };

  const test = async (account: AccountView) => {
    setTesting(key(account));
    setNotice(null);
    try {
      setNotice({ ok: true, text: await commands.testAccount(account.host, account.username) });
    } catch (e) {
      const v = toErrorView(e);
      setNotice({ ok: false, text: v.fix ? `${v.message} ${v.fix}` : v.message });
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
      {S.vault.add}
    </button>
  );

  return (
    <div className="vault">
      <ScreenHeader
        title={S.vault.title}
        subtitle={S.vault.subtitle}
        actions={form.mode === "closed" && !view?.keychain_problem && addButton}
      />
      <p className="notice notice--info">{S.vault.reminder}</p>
      {view?.keychain_problem && <ErrorNotice error={view.keychain_problem} />}
      {error && <ErrorNotice error={error} />}

      {form.mode !== "closed" && (
        <section className="card">
          <AccountForm
            key={form.mode === "edit" ? key(form.account) : "new"}
            editing={form.mode === "edit" ? form.account : null}
            onCancel={() => {
              setForm({ mode: "closed" });
            }}
            onSave={async (host, username, password) => {
              await run(() => commands.saveAccount(host, username, password)).then(
                () => {
                  setForm({ mode: "closed" });
                },
                () => undefined,
              );
            }}
          />
        </section>
      )}

      {view && view.accounts.length === 0 && form.mode === "closed" && !view.keychain_problem && (
        <div className="card">
          <div className="empty">
            <h2 className="empty__title">{S.vault.emptyTitle}</h2>
            <p className="empty__body">{S.vault.emptyBody}</p>
            {addButton}
          </div>
        </div>
      )}

      {view && view.accounts.length > 0 && (
        <div className="card">
          <table className="table">
            <thead>
              <tr>
                <th scope="col">{S.vault.host}</th>
                <th scope="col">{S.vault.username}</th>
                <th scope="col">{S.vault.password}</th>
                <th scope="col">
                  <span className="visually-hidden">{S.vault.test}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {view.accounts.map((account) => (
                <tr key={key(account)}>
                  <td className="mono">{account.host}</td>
                  <td className="mono">{account.username}</td>
                  <td>{account.has_password ? S.vault.stored : S.vault.missing}</td>
                  <td className="num">
                    <div className="row row--end">
                      <button
                        type="button"
                        className="btn btn--sm"
                        disabled={testing !== null}
                        onClick={() => {
                          void test(account);
                        }}
                      >
                        {S.vault.test}
                      </button>
                      <button
                        type="button"
                        className="btn btn--sm"
                        onClick={() => {
                          setForm({ mode: "edit", account });
                        }}
                      >
                        {S.vault.edit}
                      </button>
                      <button
                        type="button"
                        className="btn btn--sm btn--danger"
                        onClick={() => {
                          setRemoving(account);
                        }}
                      >
                        {S.vault.remove}
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {notice && (
        <p className={`notice ${notice.ok ? "notice--ok" : "notice--error"}`} role="status">
          {notice.text}
        </p>
      )}

      <Modal
        open={removing !== null}
        title={S.vault.removeTitle}
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
                  run(() => commands.removeAccount(target.host, target.username)).catch(
                    () => undefined,
                  );
                }
              }}
            >
              {S.vault.remove}
            </button>
          </>
        }
      >
        <p>{removing ? S.vault.removeBody(removing.username) : ""}</p>
      </Modal>
    </div>
  );
}
