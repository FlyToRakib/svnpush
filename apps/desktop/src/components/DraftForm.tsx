import { useState, type SyntheticEvent } from "react";
import type { DraftContext } from "../ipc/bindings/DraftContext";
import type { ReleaseDraft } from "../ipc/bindings/ReleaseDraft";
import { S } from "../strings";

interface DraftFormProps {
  context: DraftContext;
  /** The AI's draft when one arrived, otherwise the pre-filled one. */
  initial: ReleaseDraft;
  /** Goes up each time a new AI draft arrives. */
  generation: number;
  onApprove: (draft: ReleaseDraft) => void;
  disabled: boolean;
}

/** Step 2: the version, changelog entry and upgrade notice to approve. */
export function DraftForm({ context, initial, generation, onApprove, disabled }: DraftFormProps) {
  const [draft, setDraftState] = useState<ReleaseDraft>(initial);
  const [edited, setEdited] = useState(false);
  const [shown, setShown] = useState(generation);

  // A new AI draft fills an untouched form. Once you have typed, it waits to be asked for.
  const offered = generation !== shown;
  if (offered && !edited) {
    setShown(generation);
    setDraftState(initial);
  }
  const setDraft = (next: ReleaseDraft) => {
    setDraftState(next);
    setEdited(true);
  };
  const takeOffered = () => {
    setShown(generation);
    setDraftState(initial);
    setEdited(false);
  };

  const submit = (event: SyntheticEvent) => {
    event.preventDefault();
    onApprove(draft);
  };

  return (
    <form className="stack" onSubmit={submit} aria-label={S.draft.title}>
      <h3 className="section-title">{S.draft.title}</h3>
      {offered && edited && (
        <div className="notice notice--info row" role="status">
          <span>{S.draft.aiReady}</span>
          <button type="button" className="btn btn--sm" onClick={takeOffered}>
            {S.draft.useAi}
          </button>
        </div>
      )}
      <div className="field">
        <label className="field__label" htmlFor="draft-version">
          {S.draft.version}
        </label>
        <input
          id="draft-version"
          className="input input--short mono"
          value={draft.version}
          onChange={(e) => {
            setDraft({ ...draft, version: e.target.value });
          }}
          required
        />
        <p className="field__hint">{S.draft.versionHint(context.previous)}</p>
        {draft.reason && <p className="field__hint">{S.draft.reason(draft.reason)}</p>}
      </div>
      <div className="field">
        <label className="field__label" htmlFor="draft-changelog">
          {S.draft.changelog}
        </label>
        <textarea
          id="draft-changelog"
          className="textarea"
          value={draft.changelog_markdown}
          onChange={(e) => {
            setDraft({ ...draft, changelog_markdown: e.target.value });
          }}
          rows={8}
          required
        />
        <p className="field__hint">{S.draft.changelogHint}</p>
      </div>
      <div className="field">
        <label className="field__label" htmlFor="draft-notice">
          {S.draft.upgradeNotice}
        </label>
        <textarea
          id="draft-notice"
          className="textarea textarea--short"
          value={draft.upgrade_notice}
          onChange={(e) => {
            setDraft({ ...draft, upgrade_notice: e.target.value });
          }}
          rows={3}
        />
        <p className="field__hint">{S.draft.upgradeNoticeHint}</p>
      </div>
      {(draft.summary || draft.provider) && (
        <div className="field">
          <label className="field__label" htmlFor="draft-summary">
            {S.draft.summary}
          </label>
          <textarea
            id="draft-summary"
            className="textarea textarea--short"
            value={draft.summary}
            onChange={(e) => {
              setDraft({ ...draft, summary: e.target.value });
            }}
            rows={3}
          />
          <p className="field__hint">{S.draft.summaryHint}</p>
        </div>
      )}
      <div>
        <button type="submit" className="btn btn--primary" disabled={disabled}>
          {S.draft.approve}
        </button>
      </div>
    </form>
  );
}
