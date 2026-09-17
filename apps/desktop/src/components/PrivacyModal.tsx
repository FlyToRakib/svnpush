import type { PrivacyNotice } from "../ipc/bindings/PrivacyNotice";
import { S } from "../strings";
import { Modal } from "./Modal";

interface PrivacyModalProps {
  notice: PrivacyNotice;
  onAccept: () => void;
  onDecline: () => void;
}

/** The one-time notice before a provider first receives project data (plan §9.8). */
export function PrivacyModal({ notice, onAccept, onDecline }: PrivacyModalProps) {
  return (
    <Modal
      open
      title={S.ai.privacyTitle}
      onClose={onDecline}
      actions={
        <>
          <button type="button" className="btn" onClick={onDecline}>
            {S.ai.writeMyself}
          </button>
          <button type="button" className="btn btn--primary" onClick={onAccept}>
            {S.ai.privacyAccept}
          </button>
        </>
      }
    >
      <p>{S.ai.privacyIntro(notice.provider.label)}</p>
      <ul className="bullets">
        {S.ai.privacySends.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>
      <p>{S.ai.privacyNever}</p>
      {notice.revoye && <p>{S.ai.privacyRevoye}</p>}
    </Modal>
  );
}
