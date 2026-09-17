import { useState } from "react";
import type { ReleaseDraft } from "../ipc/bindings/ReleaseDraft";
import type { RunState } from "../ipc/bindings/RunState";
import type { Step } from "../ipc/bindings/Step";
import { S } from "../strings";
import { ChangeList } from "./ChangeList";
import { CheckTable } from "./CheckTable";
import { DetectSummary } from "./DetectSummary";
import { DiffView } from "./DiffView";
import { DraftForm } from "./DraftForm";
import { FileList } from "./FileList";
import { PublishPanel } from "./PublishPanel";
import { StepCard } from "./StepCard";
import { SvnPreviewView } from "./SvnPreviewView";

interface ChecklistProps {
  state: RunState;
  /** Disables the draft form, for example once the run has ended. */
  disabled: boolean;
  onApprove: (draft: ReleaseDraft) => void;
  onPublish: (trunkMessage: string, tagMessage: string) => void;
}

/** The seven-step release checklist. Each card expands to its content. */
export function Checklist({ state, disabled, onApprove, onPublish }: ChecklistProps) {
  const [toggled, setToggled] = useState<Partial<Record<Step, boolean>>>({});

  const current = state.steps.find((s) => ["Running", "Waiting", "Failed"].includes(s.status));
  const focus: Step | undefined =
    current?.step ?? (state.publish ? "Publish" : state.preview ? "Preview" : undefined);
  const isExpanded = (step: Step) => toggled[step] ?? step === focus;

  const content = (step: Step) => {
    switch (step) {
      case "Detect":
        return <DetectSummary state={state} />;
      case "Draft":
        return (
          state.draft_context && (
            <div className="stack">
              <ChangeList changes={state.draft_context.changes} />
              {state.phase === "AwaitingApproval" ? (
                <DraftForm
                  key={state.id}
                  context={state.draft_context}
                  onApprove={onApprove}
                  disabled={disabled}
                />
              ) : (
                state.draft && (
                  <div className="stack">
                    <p>{S.draft.approved(state.draft.version)}</p>
                    <pre className="prose-block">{state.draft.changelog_markdown}</pre>
                  </div>
                )
              )}
            </div>
          )
        );
      case "Write":
        return state.diffs.length > 0 ? (
          <div className="stack">
            {state.diffs.map((d) => (
              <DiffView key={d.path} diff={d.diff} label={d.path} />
            ))}
          </div>
        ) : (
          state.steps.find((s) => s.step === "Write")?.status === "Done" && (
            <p className="muted">{S.write.none}</p>
          )
        );
      case "Verify":
        return state.checks.length > 0 && <CheckTable checks={state.checks} />;
      case "Build":
        return state.package && <FileList pkg={state.package} />;
      case "Preview":
        return state.preview && <SvnPreviewView preview={state.preview} />;
      case "Publish":
        return <PublishPanel key={state.id} state={state} onPublish={onPublish} />;
    }
  };

  return (
    <ol className="checklist">
      {state.steps.map((view) => (
        <StepCard
          key={view.step}
          view={view}
          expanded={isExpanded(view.step)}
          onToggle={() => {
            setToggled({ ...toggled, [view.step]: !isExpanded(view.step) });
          }}
        >
          {content(view.step)}
        </StepCard>
      ))}
    </ol>
  );
}
