import type { ProviderRecord } from "../ipc/bindings/ProviderRecord";
import { S } from "../strings";

interface ProviderRowProps {
  record: ProviderRecord;
  busy: boolean;
  onTest: () => void;
  onDefault: () => void;
  onEdit: () => void;
  onRemove: () => void;
  onClear: () => void;
}

/** One provider: label, default badge, kind and model, attention, usage and actions. */
export function ProviderRow({
  record,
  busy,
  onTest,
  onDefault,
  onEdit,
  onRemove,
  onClear,
}: ProviderRowProps) {
  return (
    <li className="provider">
      <div className="provider__icon" aria-hidden="true">
        {record.label.slice(0, 1).toUpperCase()}
      </div>
      <div className="provider__info">
        <div className="provider__name">
          {record.label}
          {record.is_default && <span className="badge">{S.providers.defaultBadge}</span>}
        </div>
        <div className="provider__meta mono">
          {record.kind} · {record.model}
        </div>
        {record.needs_attention && (
          <div className="provider__attention">
            {S.providers.needsAttention(record.needs_attention)}
          </div>
        )}
      </div>
      <div className="provider__usage">{S.providers.requests(record.requests_this_month)}</div>
      <div className="provider__actions">
        {record.needs_attention && (
          <button type="button" className="btn btn--sm" onClick={onClear} disabled={busy}>
            {S.providers.clear}
          </button>
        )}
        <button type="button" className="btn btn--sm" onClick={onTest} disabled={busy}>
          {S.providers.test}
        </button>
        {!record.is_default && (
          <button type="button" className="btn btn--sm" onClick={onDefault} disabled={busy}>
            {S.providers.setDefault}
          </button>
        )}
        <button type="button" className="btn btn--sm" onClick={onEdit} disabled={busy}>
          {S.providers.edit}
        </button>
        <button
          type="button"
          className="btn btn--sm btn--danger"
          onClick={onRemove}
          disabled={busy}
        >
          {S.providers.remove}
        </button>
      </div>
    </li>
  );
}
