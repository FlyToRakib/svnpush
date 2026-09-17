import type { Decision } from "../ipc/bindings/Decision";
import type { DraftAi } from "../ipc/bindings/DraftAi";
import { S } from "../strings";
import { AiTaskView } from "./AiTaskView";

interface DraftAiPanelProps {
  ai: DraftAi;
  disabled: boolean;
  onDecide: (decision: Decision) => void;
  onOpenProviders: () => void;
}

/** Step 2's AI panel: which provider runs, its progress, and notes about the draft. */
export function DraftAiPanel({ ai, disabled, onDecide, onOpenProviders }: DraftAiPanelProps) {
  const fellBackFrom = ai.draft?.fell_back_from ?? null;
  const fellBackLabel =
    fellBackFrom && (ai.task.choices.find((c) => c.id === fellBackFrom)?.label ?? fellBackFrom);
  const showNotes = ai.task.status === "Done";
  return (
    <section className="stack ai-panel" aria-label={S.ai.draftTitle}>
      <h3 className="section-title">{S.ai.draftTitle}</h3>
      <AiTaskView
        task={ai.task}
        disabled={disabled}
        askLabel={S.ai.askAi}
        stopLabel={S.ai.stop}
        doneLabel={S.ai.drafted}
        onDecide={onDecide}
        onOpenProviders={onOpenProviders}
      />
      {showNotes && fellBackLabel && (
        <p className="notice notice--info">{S.ai.fellBack(fellBackLabel)}</p>
      )}
      {showNotes && ai.from_summaries && (
        <p className="notice notice--info">{S.ai.fromSummaries}</p>
      )}
      {showNotes && ai.notice && <p className="notice notice--warn">{ai.notice}</p>}
    </section>
  );
}
