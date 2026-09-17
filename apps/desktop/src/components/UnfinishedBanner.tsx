import type { RunJournal } from "../ipc/bindings/RunJournal";
import { S } from "../strings";

const STEP_NUMBERS = {
  Detect: 1,
  Draft: 2,
  Write: 3,
  Verify: 4,
  Build: 5,
  Preview: 6,
  Publish: 7,
} as const;

interface UnfinishedBannerProps {
  journal: RunJournal;
  disabled: boolean;
  onResume: () => void;
  onDiscard: () => void;
}

/** An interrupted run, or a trunk commit whose tag is missing, with Resume and Discard. */
export function UnfinishedBanner({
  journal,
  disabled,
  onResume,
  onDiscard,
}: UnfinishedBannerProps) {
  const needsTag = journal.revisions.trunk !== null && journal.revisions.tag === null;
  const last = journal.steps.at(-1)?.step;
  const message = needsTag
    ? S.release.needsTag(journal.version ?? "")
    : S.release.interrupted(last ? STEP_NUMBERS[last] : 1);
  return (
    <div className="notice notice--warn banner" role="status">
      <p>{message}</p>
      <div className="row">
        <button type="button" className="btn btn--sm" disabled={disabled} onClick={onResume}>
          {needsTag ? S.release.resumeTag : S.release.resume}
        </button>
        {!needsTag && (
          <button type="button" className="btn btn--sm" disabled={disabled} onClick={onDiscard}>
            {S.release.discard}
          </button>
        )}
      </div>
    </div>
  );
}
