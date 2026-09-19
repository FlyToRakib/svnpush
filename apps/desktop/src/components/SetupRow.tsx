import type { ReactNode } from "react";
import { S } from "../strings";

export type SetupStatus = "ready" | "todo" | "optional";

interface SetupRowProps {
  title: string;
  status: SetupStatus;
  children: ReactNode;
}

const LABELS: Record<SetupStatus, string> = {
  ready: S.help.ready,
  todo: S.help.todo,
  optional: S.help.optional,
};

const PILLS: Record<SetupStatus, string> = { ready: "pass", todo: "fail", optional: "skip" };

/** One item of the setup checklist: its name, its state and what to do. */
export function SetupRow({ title, status, children }: SetupRowProps) {
  return (
    <li className="setup__row">
      <div className="row row--between">
        <strong>{title}</strong>
        <span className={`pill pill--${PILLS[status]}`}>{LABELS[status]}</span>
      </div>
      <div className="stack">{children}</div>
    </li>
  );
}
