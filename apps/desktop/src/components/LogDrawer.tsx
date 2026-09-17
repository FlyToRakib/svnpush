import { useEffect, useRef, useState } from "react";
import type { LogLine } from "../ipc/bindings/LogLine";
import { S } from "../strings";
import { CopyButton } from "./CopyButton";

interface LogDrawerProps {
  logs: LogLine[];
}

/** The collapsible drawer that streams command output. */
export function LogDrawer({ logs }: LogDrawerProps) {
  const [open, setOpen] = useState(false);
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) {
      end.current?.scrollIntoView({ block: "end" });
    }
  }, [logs.length, open]);

  const text = logs.map((l) => (l.stream === "Command" ? `$ ${l.text}` : l.text)).join("\n");

  return (
    <section className={`log${open ? " log--open" : ""}`} aria-label={S.log.title}>
      <div className="log__bar">
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          aria-expanded={open}
          aria-controls="log-body"
          onClick={() => {
            setOpen(!open);
          }}
        >
          {open ? S.log.hide : S.log.show} ({logs.length})
        </button>
        {open && logs.length > 0 && <CopyButton text={text} label={S.log.copy} />}
      </div>
      {open && (
        <div id="log-body" className="log__body" role="log" aria-live="polite" tabIndex={0}>
          {logs.length === 0 ? (
            <p className="muted">{S.log.empty}</p>
          ) : (
            logs.map((line, index) => (
              <div key={index} className={`log__line log__line--${line.stream.toLowerCase()}`}>
                {line.stream === "Command" ? `$ ${line.text}` : line.text}
              </div>
            ))
          )}
          <div ref={end} />
        </div>
      )}
    </section>
  );
}
