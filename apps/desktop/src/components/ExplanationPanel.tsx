import { useState } from "react";
import type { Decision } from "../ipc/bindings/Decision";
import type { Explanation } from "../ipc/bindings/Explanation";
import { S } from "../strings";
import { AiTaskView } from "./AiTaskView";
import { DiffView } from "./DiffView";

interface ExplanationPanelProps {
  explanation: Explanation;
  /** Whether the run is waiting for fixes or Stop. */
  waiting: boolean;
  disabled: boolean;
  onDecide: (decision: Decision) => void;
  onOpenProviders: () => void;
}

/** Step 4 after a blocking failure: the AI's explanation and readme fixes to apply. */
export function ExplanationPanel({
  explanation,
  waiting,
  disabled,
  onDecide,
  onOpenProviders,
}: ExplanationPanelProps) {
  const [selected, setSelected] = useState<number[]>([]);
  const applicable = explanation.fixes.filter((f) => f.problem === null).length;

  return (
    <section className="stack ai-panel" aria-label={S.ai.explainTitle}>
      <h3 className="section-title">{S.ai.explainTitle}</h3>
      <AiTaskView
        task={explanation.task}
        disabled={disabled}
        askLabel={S.ai.explainAgain}
        stopLabel={S.ai.stopExplaining}
        doneLabel={S.ai.explained}
        onDecide={onDecide}
        onOpenProviders={onOpenProviders}
      />
      {explanation.text && <p className="prose">{explanation.text}</p>}
      {explanation.task.status === "Done" && explanation.fixes.length === 0 && (
        <p className="muted">{S.ai.noFixes}</p>
      )}
      {explanation.fixes.length > 0 && (
        <fieldset className="fieldset stack">
          <legend className="field__label">{S.ai.fixesTitle}</legend>
          {explanation.fixes.map((fix, index) => (
            <div key={`${fix.check_id}-${String(index)}`} className="fix stack">
              <label className="checkbox">
                <input
                  type="checkbox"
                  disabled={fix.problem !== null || disabled}
                  checked={selected.includes(index)}
                  onChange={(e) => {
                    setSelected(
                      e.target.checked ? [...selected, index] : selected.filter((i) => i !== index),
                    );
                  }}
                />
                {S.ai.fixFor(fix.check_id)} <span className="mono">{fix.path}</span>
              </label>
              {fix.problem ? (
                <p className="muted">
                  {S.ai.cannotApply} {fix.problem}
                </p>
              ) : (
                <DiffView diff={fix.diff} label={fix.path} />
              )}
            </div>
          ))}
        </fieldset>
      )}
      {waiting && (
        <div className="row">
          {applicable > 0 && (
            <button
              type="button"
              className="btn btn--primary"
              disabled={disabled || selected.length === 0}
              onClick={() => {
                onDecide({ kind: "ApplyFixes", fixes: selected });
              }}
            >
              {S.ai.applyFixes}
            </button>
          )}
          <button
            type="button"
            className="btn btn--danger"
            disabled={disabled}
            onClick={() => {
              onDecide({ kind: "Stop" });
            }}
          >
            {S.ai.stopRelease}
          </button>
        </div>
      )}
    </section>
  );
}
