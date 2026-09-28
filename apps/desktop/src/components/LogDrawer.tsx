import { useEffect, useRef, useState } from "react";
import { useRunStore, type LogEntry } from "../store/runStore";
import { S } from "../strings";
import { CopyButton } from "./CopyButton";

const NO_LOGS: LogEntry[] = [];

/** How close to the end, in pixels, still counts as following the output. */
const FOLLOW_SLACK = 40;

const lineText = (line: LogEntry) => (line.stream === "Command" ? `$ ${line.text}` : line.text);

interface LogDrawerProps {
  projectPath: string;
}

/** The collapsible drawer that streams command output. */
export function LogDrawer({ projectPath }: LogDrawerProps) {
  const logs = useRunStore((s) => s.runs[projectPath]?.logs ?? NO_LOGS);
  const [open, setOpen] = useState(false);
  const body = useRef<HTMLDivElement>(null);
  // Follow new output only while the reader is at the end, not while they scroll back.
  const following = useRef(true);

  useEffect(() => {
    const element = body.current;
    if (open && element && following.current) {
      element.scrollTop = element.scrollHeight;
    }
  }, [logs, open]);

  return (
    <section className={`log${open ? " log--open" : ""}`} aria-label={S.log.title}>
      <div className="log__bar">
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          aria-expanded={open}
          aria-controls="log-body"
          onClick={() => {
            following.current = true;
            setOpen(!open);
          }}
        >
          {open ? S.log.hide : S.log.show} ({logs.length})
        </button>
        {open && logs.length > 0 && (
          <CopyButton text={() => logs.map(lineText).join("\n")} label={S.log.copy} />
        )}
      </div>
      {open && (
        <div
          id="log-body"
          ref={body}
          className="log__body"
          role="log"
          aria-live="polite"
          tabIndex={0}
          onScroll={(e) => {
            const el = e.currentTarget;
            following.current = el.scrollHeight - el.scrollTop - el.clientHeight < FOLLOW_SLACK;
          }}
        >
          {logs.length === 0 ? (
            <p className="muted">{S.log.empty}</p>
          ) : (
            logs.map((line) => (
              <div key={line.seq} className={`log__line log__line--${line.stream.toLowerCase()}`}>
                {lineText(line)}
              </div>
            ))
          )}
        </div>
      )}
    </section>
  );
}
