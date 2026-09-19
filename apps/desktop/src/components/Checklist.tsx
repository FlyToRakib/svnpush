import { useState } from "react";
import type { Decision } from "../ipc/bindings/Decision";
import type { ReleaseDraft } from "../ipc/bindings/ReleaseDraft";
import type { RunState } from "../ipc/bindings/RunState";
import type { Step } from "../ipc/bindings/Step";
import { S } from "../strings";
import { ChangeList } from "./ChangeList";
import { CheckTable } from "./CheckTable";
import { DetectSummary } from "./DetectSummary";
import { DiffView } from "./DiffView";
import { DraftAiPanel } from "./DraftAiPanel";
import { DraftForm } from "./DraftForm";
import { ExplanationPanel } from "./ExplanationPanel";
import { FileList } from "./FileList";
import { FileReviewPanel } from "./FileReviewPanel";
import { PublishPanel } from "./PublishPanel";
import { StepCard } from "./StepCard";
import { SvnPreviewView } from "./SvnPreviewView";

interface ChecklistProps {
  state: RunState;
  /** Disables the forms, for example once the run has ended. */
  disabled: boolean;
  onApprove: (draft: ReleaseDraft) => void;
  onPublish: (trunkMessage: string, tagMessage: string) => void;
  onDecide: (decision: Decision) => void;
  onOpenProviders: () => void;
  onConfirmFiles: (distignore: string | null) => void;
}

/** The seven-step release checklist. Each card expands to its content. */
export function Checklist({
  state,
  disabled,
  onApprove,
  onPublish,
  onDecide,
  onOpenProviders,
  onConfirmFiles,
}: ChecklistProps) {
  const [toggled, setToggled] = useState<Partial<Record<Step, boolean>>>({});

  const current = state.steps.find((s) => ["Running", "Waiting", "Failed"].includes(s.status));
  const focus: Step | undefined =
    current?.step ?? (state.publish ? "Publish" : state.preview ? "Preview" : undefined);
  const isExpanded = (step: Step) => toggled[step] ?? step === focus;
  const drafting = state.phase === "AwaitingApproval" || state.phase === "Drafting";

  const content = (step: Step) => {
    switch (step) {
      case "Detect":
        return <DetectSummary state={state} />;
      case "Draft":
        return (
          state.draft_context && (
            <div className="stack">
              <ChangeList changes={state.draft_context.changes} />
              {drafting && state.draft_ai && (
                <DraftAiPanel
                  ai={state.draft_ai}
                  disabled={disabled}
                  onDecide={onDecide}
                  onOpenProviders={onOpenProviders}
                />
              )}
              {drafting ? (
                <DraftForm
                  key={`${state.id}-${String(state.draft_ai?.generation ?? 0)}`}
                  context={state.draft_context}
                  initial={state.draft_ai?.draft ?? state.draft_context.prefill}
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
            {state.diffs.map((d, index) => (
              <DiffView key={`${d.path}-${String(index)}`} diff={d.diff} label={d.path} />
            ))}
          </div>
        ) : (
          state.steps.find((s) => s.step === "Write")?.status === "Done" && (
            <p className="muted">{S.write.none}</p>
          )
        );
      case "Verify":
        return (
          (state.checks.length > 0 || state.explanation) && (
            <div className="stack">
              {state.explanation && (
                <ExplanationPanel
                  key={`${state.id}-${String(state.diffs.length)}`}
                  explanation={state.explanation}
                  waiting={state.phase === "AwaitingFixes"}
                  disabled={disabled}
                  onDecide={onDecide}
                  onOpenProviders={onOpenProviders}
                />
              )}
              <CheckTable checks={state.checks} />
            </div>
          )
        );
      case "Build":
        return state.phase === "AwaitingFileReview" && state.file_review ? (
          <FileReviewPanel
            key={state.id}
            projectPath={state.project_path}
            review={state.file_review}
            disabled={disabled}
            onConfirm={onConfirmFiles}
          />
        ) : (
          state.package && <FileList pkg={state.package} />
        );
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
