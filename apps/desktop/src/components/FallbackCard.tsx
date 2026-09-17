import { useState } from "react";
import type { ProviderRecord } from "../ipc/bindings/ProviderRecord";
import { S } from "../strings";

interface FallbackCardProps {
  providers: ProviderRecord[];
  chosen: string[];
  onSave: (order: string[]) => Promise<void>;
}

interface Row {
  id: string;
  on: boolean;
}

/** The opt-in fallback chain: an ordered checklist, reordered with buttons (keyboard friendly). */
export function FallbackCard({ providers, chosen, onSave }: FallbackCardProps) {
  const [rows, setRows] = useState<Row[]>(() => [
    ...chosen.map((id) => ({ id, on: true })),
    ...providers.filter((p) => !chosen.includes(p.id)).map((p) => ({ id: p.id, on: false })),
  ]);
  const [saved, setSaved] = useState<string | null>(null);

  const move = (index: number, by: number) => {
    const next = [...rows];
    const [row] = next.splice(index, 1);
    if (row) {
      next.splice(index + by, 0, row);
      setRows(next);
      setSaved(null);
    }
  };

  const save = async () => {
    const order = rows.filter((r) => r.on).map((r) => r.id);
    await onSave(order);
    setSaved(S.providers.fallbackSaved(order.length));
  };

  return (
    <details className="card fallback">
      <summary className="card__header card__summary">
        <h2 className="card__title">{S.providers.fallbackTitle}</h2>
      </summary>
      <div className="card__body stack">
        <p className="field__hint">{S.providers.fallbackBody}</p>
        <ol className="fallback__list">
          {rows.map((row, index) => {
            const record = providers.find((p) => p.id === row.id);
            if (!record) {
              return null;
            }
            const rank = rows.filter((r) => r.on).findIndex((r) => r.id === row.id);
            return (
              <li key={row.id} className="fallback__row">
                <span className="fallback__rank">{row.on ? `${String(rank + 1)}.` : "—"}</span>
                <label className="checkbox fallback__name">
                  <input
                    type="checkbox"
                    checked={row.on}
                    onChange={(e) => {
                      setRows(
                        rows.map((r) => (r.id === row.id ? { ...r, on: e.target.checked } : r)),
                      );
                      setSaved(null);
                    }}
                  />
                  {record.label}
                </label>
                <span className="mono muted">{record.model}</span>
                <button
                  type="button"
                  className="btn btn--sm btn--ghost"
                  aria-label={`${S.providers.moveUp}: ${record.label}`}
                  disabled={index === 0}
                  onClick={() => {
                    move(index, -1);
                  }}
                >
                  ↑
                </button>
                <button
                  type="button"
                  className="btn btn--sm btn--ghost"
                  aria-label={`${S.providers.moveDown}: ${record.label}`}
                  disabled={index === rows.length - 1}
                  onClick={() => {
                    move(index, 1);
                  }}
                >
                  ↓
                </button>
              </li>
            );
          })}
        </ol>
        {saved && <p className="notice notice--ok">{saved}</p>}
        <div>
          <button
            type="button"
            className="btn btn--sm"
            onClick={() => {
              void save();
            }}
          >
            {S.providers.saveFallback}
          </button>
        </div>
      </div>
    </details>
  );
}
