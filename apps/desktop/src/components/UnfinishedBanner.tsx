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

/**
 * An interrupted run, a trunk commit whose tag is missing, or a publish that
 * stopped before the server confirmed it, with Resume and Discard.
 */
export function UnfinishedBanner({
  journal,
  disabled,
  onResume,
  onDiscard,
}: UnfinishedBannerProps) {
  // Mirrors `RunJournal::needs_tag`: a trunk commit, or a commit or copy that
  // may have landed, with no tag recorded.
  const unconfirmed =
    !journal.assets_only &&
    !journal.dry_run &&
    !journal.discarded &&
    journal.revisions.trunk === null &&
    journal.in_flight?.kind === "Commit";
  const needsTag =
    (journal.revisions.trunk !== null || journal.in_flight != null) &&
    journal.revisions.tag === null &&
    !journal.dry_run &&
    !journal.discarded &&
    !journal.assets_only;
  const last = journal.steps.at(-1)?.step;
  const version = journal.version ?? "";
  const message = unconfirmed
    ? S.tools.inFlight(version)
    : needsTag
      ? S.release.needsTag(version)
      : S.release.interrupted(last ? STEP_NUMBERS[last] : 1);
  return (
    <div className="notice notice--warn banner" role="status">
      <p>{message}</p>
      <div className="row">
        <button type="button" className="btn btn--sm" disabled={disabled} onClick={onResume}>
          {unconfirmed ? S.tools.resumeInFlight : needsTag ? S.release.resumeTag : S.release.resume}
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
