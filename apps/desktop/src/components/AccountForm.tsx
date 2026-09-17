import { useState, type SyntheticEvent } from "react";
import type { AccountView } from "../ipc/bindings/AccountView";
import { S } from "../strings";

/** The WordPress.org plugin SVN host, pre-filled for new accounts. */
export const WORDPRESS_SVN_HOST = "plugins.svn.wordpress.org";

interface AccountFormProps {
  editing: AccountView | null;
  onSave: (host: string, username: string, password: string) => Promise<void>;
  onCancel: () => void;
}

/** Host, username and a write-only password. */
export function AccountForm({ editing, onSave, onCancel }: AccountFormProps) {
  const [host, setHost] = useState(editing?.host ?? WORDPRESS_SVN_HOST);
  const [username, setUsername] = useState(editing?.username ?? "");
  const [password, setPassword] = useState("");
  const [show, setShow] = useState(false);

  const submit = async (event: SyntheticEvent) => {
    event.preventDefault();
    await onSave(host, username, password);
    setPassword("");
  };

  return (
    <form
      className="card__body stack"
      onSubmit={(e) => {
        void submit(e);
      }}
    >
      <div className="grid-2">
        <div className="field">
          <label className="field__label" htmlFor="acc-host">
            {S.vault.host}
          </label>
          <input
            id="acc-host"
            className="input mono"
            value={host}
            readOnly={editing !== null}
            onChange={(e) => {
              setHost(e.target.value);
            }}
          />
        </div>
        <div className="field">
          <label className="field__label" htmlFor="acc-user">
            {S.vault.username}
          </label>
          <input
            id="acc-user"
            className="input mono"
            autoComplete="off"
            value={username}
            readOnly={editing !== null}
            onChange={(e) => {
              setUsername(e.target.value);
            }}
          />
        </div>
      </div>
      <div className="field">
        <label className="field__label" htmlFor="acc-pass">
          {S.vault.password}
        </label>
        <div className="input-group">
          <input
            id="acc-pass"
            className="input mono"
            type={show ? "text" : "password"}
            autoComplete="off"
            value={password}
            placeholder={
              editing?.has_password
                ? S.vault.passwordPlaceholderExisting
                : S.vault.passwordPlaceholderNew
            }
            onChange={(e) => {
              setPassword(e.target.value);
            }}
          />
          <button
            type="button"
            className="btn"
            aria-pressed={show}
            onClick={() => {
              setShow(!show);
            }}
          >
            {show ? S.vault.hidePassword : S.vault.showPassword}
          </button>
        </div>
        <p className="field__hint">{S.vault.passwordHint}</p>
      </div>
      <div className="row">
        <button type="submit" className="btn btn--primary">
          {S.vault.save}
        </button>
        <button type="button" className="btn btn--ghost" onClick={onCancel}>
          {S.common.cancel}
        </button>
      </div>
    </form>
  );
}
