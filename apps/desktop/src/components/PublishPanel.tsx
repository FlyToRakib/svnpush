import { useState } from "react";
import { formatDelta } from "../format";
import type { RunState } from "../ipc/bindings/RunState";
import { openExternal } from "../opener";
import { S } from "../strings";
import { Modal } from "./Modal";

interface PublishPanelProps {
  state: RunState;
  onPublish: (trunkMessage: string, tagMessage: string) => void;
}

/** Step 7: the commit messages, the explicit confirmation, and the result. */
export function PublishPanel({ state, onPublish }: PublishPanelProps) {
  const preview = state.preview;
  const [trunkMessage, setTrunkMessage] = useState(preview?.trunk_message ?? "");
  const [tagMessage, setTagMessage] = useState(preview?.tag_message ?? "");
  const [confirming, setConfirming] = useState(false);

  const result = state.publish;
  if (result) {
    const verification = result.verification;
    return (
      <div className="stack">
        <dl className="facts">
          <dt>{S.publish.trunkRevision}</dt>
          <dd className="mono">
            {result.trunk_revision !== null
              ? `r${String(result.trunk_revision)}`
              : S.publish.unchanged}
          </dd>
          <dt>{S.publish.tagRevision}</dt>
          <dd className="mono">
            {result.tag_revision !== null ? `r${String(result.tag_revision)}` : "—"}
          </dd>
        </dl>
        {verification && (
          <p
            className={`notice ${verification.state === "Verified" ? "notice--ok" : "notice--warn"}`}
          >
            {verification.state === "Verified"
              ? S.publish.verified
              : S.publish.unverified(verification.reason)}
          </p>
        )}
        <div className="row">
          <button
            type="button"
            className="btn"
            onClick={() => {
              void openExternal(result.plugin_url);
            }}
          >
            {S.publish.openPage}
          </button>
        </div>
      </div>
    );
  }

  if (state.phase !== "AwaitingPublish" || !preview) {
    return null;
  }

  const version = state.draft?.version ?? "";
  return (
    <div className="stack">
      <div className="field">
        <label className="field__label" htmlFor="trunk-message">
          {S.preview.trunkMessage}
        </label>
        <input
          id="trunk-message"
          className="input"
          value={trunkMessage}
          onChange={(e) => {
            setTrunkMessage(e.target.value);
          }}
        />
      </div>
      <div className="field">
        <label className="field__label" htmlFor="tag-message">
          {S.preview.tagMessage}
        </label>
        <input
          id="tag-message"
          className="input"
          value={tagMessage}
          onChange={(e) => {
            setTagMessage(e.target.value);
          }}
        />
      </div>
      <p className="notice notice--warn">{S.publish.confirmBody}</p>
      <div>
        <button
          type="button"
          className="btn btn--primary"
          disabled={!trunkMessage.trim() || !tagMessage.trim()}
          onClick={() => {
            setConfirming(true);
          }}
        >
          {S.publish.publish}
        </button>
      </div>
      <Modal
        open={confirming}
        title={S.publish.confirmTitle}
        onClose={() => {
          setConfirming(false);
        }}
        actions={
          <>
            <button
              type="button"
              className="btn"
              onClick={() => {
                setConfirming(false);
              }}
            >
              {S.common.cancel}
            </button>
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => {
                setConfirming(false);
                onPublish(trunkMessage, tagMessage);
              }}
            >
              {S.publish.publish}
            </button>
          </>
        }
      >
        <dl className="facts">
          <dt>{S.publish.version}</dt>
          <dd className="mono">{version}</dd>
          <dt>{S.publish.slug}</dt>
          <dd className="mono">{state.facts?.slug}</dd>
          <dt>{S.publish.svnUrl}</dt>
          <dd className="mono break">{preview.svn_url}</dd>
          <dt>{S.publish.account}</dt>
          <dd className="mono">{preview.account ?? S.release.noAccount}</dd>
        </dl>
        <p className="mono">
          {S.publish.counts(formatDelta(preview.trunk), formatDelta(preview.assets))}
        </p>
        <p>{S.publish.confirmBody}</p>
      </Modal>
    </div>
  );
}
