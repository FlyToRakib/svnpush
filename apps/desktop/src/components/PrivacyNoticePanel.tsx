import { useEffect, useId, useRef } from "react";
import type { PrivacyNotice } from "../ipc/bindings/PrivacyNotice";
import { S } from "../strings";

interface PrivacyNoticePanelProps {
  notice: PrivacyNotice;
  onAccept: () => void;
  onDecline: () => void;
}

/**
 * The one-time notice before a provider first receives project data (plan §9.8).
 * Shown inline, not as a dialog: dialogs are kept for Publish and destructive
 * actions (plan §11.2). Focus moves to it so keyboard users meet it first.
 */
export function PrivacyNoticePanel({ notice, onAccept, onDecline }: PrivacyNoticePanelProps) {
  const titleId = useId();
  const title = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    title.current?.focus();
  }, []);

  return (
    <section className="notice notice--info stack privacy" aria-labelledby={titleId}>
      <h4 id={titleId} ref={title} className="privacy__title" tabIndex={-1}>
        {S.ai.privacyTitle}
      </h4>
      <p>{S.ai.privacyIntro(notice.provider.label)}</p>
      <ul className="bullets">
        {S.ai.privacySends.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>
      <p>{S.ai.privacyNever}</p>
      {notice.revoye && <p>{S.ai.privacyRevoye}</p>}
      <div className="row">
        <button type="button" className="btn btn--primary" onClick={onAccept}>
          {S.ai.privacyAccept}
        </button>
        <button type="button" className="btn" onClick={onDecline}>
          {S.ai.writeMyself}
        </button>
      </div>
    </section>
  );
}
