import type { AiTask } from "../ipc/bindings/AiTask";
import type { Decision } from "../ipc/bindings/Decision";
import { S } from "../strings";
import { ErrorNotice } from "./ErrorNotice";
import { PrivacyModal } from "./PrivacyModal";
import { ProviderChange } from "./ProviderChange";

interface AiTaskViewProps {
  task: AiTask;
  /** The run has ended or cannot take decisions. */
  disabled: boolean;
  /** Label of the button that asks the AI again after a manual choice. */
  askLabel: string;
  /** Label of the button that stops a running request. */
  stopLabel: string;
  /** What the provider line says once the answer arrived. */
  doneLabel: (provider: string) => string;
  onDecide: (decision: Decision) => void;
  onOpenProviders: () => void;
}

/** Progress, errors and the choices around one AI request (plan §12.4). */
export function AiTaskView({
  task,
  disabled,
  askLabel,
  stopLabel,
  doneLabel,
  onDecide,
  onOpenProviders,
}: AiTaskViewProps) {
  const label = task.provider?.label ?? "";
  const generate = (providerId: string | null) => {
    onDecide({ kind: "Generate", provider_id: providerId });
  };
  const change = (
    <ProviderChange
      key={task.provider?.id ?? "none"}
      choices={task.choices}
      current={task.provider}
      disabled={disabled}
      onChoose={generate}
    />
  );
  const manual = () => {
    onDecide({ kind: "Manual" });
  };

  switch (task.status) {
    case "Off":
      return <p className="muted">{S.ai.off}</p>;
    case "NoProvider":
      return (
        <div className="stack">
          {task.error && <ErrorNotice error={task.error} />}
          <div className="row">
            <button type="button" className="btn btn--sm" onClick={onOpenProviders}>
              {S.ai.openProviders}
            </button>
            {change}
          </div>
        </div>
      );
    case "NeedsConsent":
      return (
        <div className="stack">
          <p className="muted">{S.ai.consentWaiting}</p>
          {task.privacy && !disabled && (
            <PrivacyModal
              notice={task.privacy}
              onAccept={() => {
                if (task.privacy) {
                  onDecide({ kind: "AcceptPrivacy", provider_id: task.privacy.provider.id });
                }
              }}
              onDecline={manual}
            />
          )}
        </div>
      );
    case "Working": {
      const fleet = task.fleet;
      return (
        <div className="stack ai-status" aria-live="polite">
          <p>
            <span className="spinner" aria-hidden="true" /> {S.ai.working(label)}
          </p>
          {task.summaries && (
            <p className="muted">{S.ai.summaries(task.summaries.done, task.summaries.total)}</p>
          )}
          {task.job_status === "queued" && (
            <p className="muted">{S.ai.queued(task.queue_position)}</p>
          )}
          {task.job_status && task.job_status !== "queued" && (
            <p className="muted">{S.ai.jobStatus(task.job_status)}</p>
          )}
          {fleet && (
            <p className="muted mono">
              {S.ai.fleet(
                fleet.devices_online,
                fleet.devices_total,
                fleet.agents_idle,
                fleet.queue_depth,
              )}
            </p>
          )}
          {fleet && fleet.devices_online === 0 && (
            <p className="notice notice--warn">{S.ai.startDesk}</p>
          )}
          <div>
            <button type="button" className="btn btn--sm" disabled={disabled} onClick={manual}>
              {stopLabel}
            </button>
          </div>
        </div>
      );
    }
    case "Done":
      return (
        <div className="row">
          <span className="muted">{doneLabel(label)}</span>
          <button
            type="button"
            className="btn btn--link"
            disabled={disabled}
            onClick={() => {
              generate(task.provider?.id ?? null);
            }}
          >
            {S.ai.regenerate}
          </button>
          {change}
        </div>
      );
    case "Failed":
      return (
        <div className="stack">
          {task.error && <ErrorNotice error={task.error} />}
          {task.raw_text && (
            <div className="stack">
              <p className="muted">{S.ai.rawText}</p>
              <pre className="prose-block">{task.raw_text}</pre>
            </div>
          )}
          <div className="row">
            <button
              type="button"
              className="btn btn--sm"
              disabled={disabled}
              onClick={() => {
                generate(task.provider?.id ?? null);
              }}
            >
              {S.ai.retry}
            </button>
            {change}
          </div>
        </div>
      );
    case "Manual":
      return (
        <div className="row">
          <span className="muted">{S.ai.manual}</span>
          {task.choices.length > 0 && (
            <button
              type="button"
              className="btn btn--link"
              disabled={disabled}
              onClick={() => {
                generate(null);
              }}
            >
              {askLabel}
            </button>
          )}
        </div>
      );
  }
}
