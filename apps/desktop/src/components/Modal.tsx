import { useEffect, useRef, type ReactNode } from "react";

interface ModalProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  actions: ReactNode;
}

/** A native modal dialog: focus is trapped and Escape closes it. */
export function Modal({ open, title, onClose, children, actions }: ModalProps) {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) {
      return;
    }
    if (open && !dialog.open) {
      dialog.showModal();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);

  return (
    <dialog ref={ref} className="modal" aria-labelledby="modal-title" onClose={onClose}>
      <h2 id="modal-title" className="modal__title">
        {title}
      </h2>
      <div className="modal__body">{children}</div>
      <div className="modal__actions">{actions}</div>
    </dialog>
  );
}
