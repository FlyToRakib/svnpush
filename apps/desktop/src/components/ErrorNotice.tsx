import type { ErrorView } from "../ipc/bindings/ErrorView";
import { S } from "../strings";

interface ErrorNoticeProps {
  error: ErrorView;
}

/** An error with its fix. */
export function ErrorNotice({ error }: ErrorNoticeProps) {
  return (
    <div className="notice notice--error" role="alert">
      <p>{error.message}</p>
      {error.fix && (
        <p className="notice__fix">
          <strong>{S.common.fix}:</strong> {error.fix}
        </p>
      )}
    </div>
  );
}
