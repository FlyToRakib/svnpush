import { useEffect, useRef, useState } from "react";
import type { DraftProvider } from "../ipc/bindings/DraftProvider";
import { S } from "../strings";

interface ProviderChangeProps {
  choices: DraftProvider[];
  current: DraftProvider | null;
  disabled: boolean;
  onChoose: (providerId: string) => void;
}

/** The "Change" link: pick another provider for this run only. */
export function ProviderChange({ choices, current, disabled, onChoose }: ProviderChangeProps) {
  const [open, setOpen] = useState(false);
  const [chosen, setChosen] = useState(current?.id ?? choices[0]?.id ?? "");
  const change = useRef<HTMLButtonElement>(null);
  const picker = useRef<HTMLSelectElement>(null);
  const wasOpen = useRef(false);

  // The picker replaces the Change link: take focus to it, and back when it closes.
  useEffect(() => {
    if (open) {
      picker.current?.focus();
    } else if (wasOpen.current) {
      change.current?.focus();
    }
    wasOpen.current = open;
  }, [open]);

  if (choices.length === 0) {
    return null;
  }
  if (!open) {
    return (
      <button
        ref={change}
        type="button"
        className="btn btn--link"
        disabled={disabled}
        onClick={() => {
          setOpen(true);
        }}
      >
        {S.ai.change}
      </button>
    );
  }
  return (
    <span className="row">
      <select
        ref={picker}
        className="input input--short"
        aria-label={S.ai.changeLabel}
        value={chosen}
        onChange={(e) => {
          setChosen(e.target.value);
        }}
      >
        {choices.map((choice) => (
          <option key={choice.id} value={choice.id}>
            {choice.label}
          </option>
        ))}
      </select>
      <button
        type="button"
        className="btn btn--sm"
        disabled={disabled || !chosen}
        onClick={() => {
          setOpen(false);
          onChoose(chosen);
        }}
      >
        {S.ai.use}
      </button>
      <button
        type="button"
        className="btn btn--sm btn--ghost"
        onClick={() => {
          setOpen(false);
        }}
      >
        {S.common.cancel}
      </button>
    </span>
  );
}
