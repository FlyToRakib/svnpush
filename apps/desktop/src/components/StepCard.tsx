import type { ReactNode } from "react";
import type { StepView } from "../ipc/bindings/StepView";
import { S } from "../strings";

const NUMBERS: Record<StepView["step"], number> = {
  Detect: 1,
  Draft: 2,
  Write: 3,
  Verify: 4,
  Build: 5,
  Preview: 6,
  Publish: 7,
};

interface StepCardProps {
  view: StepView;
  expanded: boolean;
  onToggle: () => void;
  children?: ReactNode;
}

/** One step of the release checklist. */
export function StepCard({ view, expanded, onToggle, children }: StepCardProps) {
  const status = view.status.toLowerCase();
  const bodyId = `step-${view.step}`;
  return (
    <li className={`step step--${status}`}>
      <button
        type="button"
        className="step__header"
        aria-expanded={expanded}
        aria-controls={bodyId}
        onClick={onToggle}
      >
        <span className="step__number" aria-hidden="true">
          {NUMBERS[view.step]}
        </span>
        <span className="step__title">{S.steps[view.step]}</span>
        <span className="step__summary">{view.summary}</span>
        <span className={`step__status step__status--${status}`}>
          {S.steps.status[view.status]}
        </span>
      </button>
      {expanded && children && (
        <div id={bodyId} className="step__body">
          {children}
        </div>
      )}
    </li>
  );
}
