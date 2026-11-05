import { useEffect, useRef, type JSX, type ReactNode } from 'react';

import './ModalDialog.scss';
import { dialogKeyDown } from '../../lib/dialogKeys';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';

interface ModalDialogProps {
  /** Id of the title element, for aria-labelledby. */
  titleId: string;
  title: string;
  open: boolean;
  onClose: () => void;
  closeLabel: string;
  /** Extra class for the size of this dialog. */
  className?: string;
  /** Rendered only while open. */
  children: ReactNode;
  /** Buttons before Close in the footer. */
  footer?: ReactNode;
}

/** A modal dialog with a title bar and a Close button, over the window. */
export function ModalDialog(props: ModalDialogProps): JSX.Element {
  const { titleId, title, open, onClose, closeLabel, className, children, footer } = props;
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const element = dialog.current;
    if (element === null) {
      return;
    }
    if (open && !element.open) {
      element.showModal();
    } else if (!open && element.open) {
      element.close();
    }
  }, [open]);

  return (
    <dialog
      aria-labelledby={titleId}
      className={className === undefined ? 'modal-dialog' : `modal-dialog ${className}`}
      onClose={onClose}
      onKeyDown={dialogKeyDown(onClose)}
      ref={dialog}
    >
      {open ? (
        <div className="modal-dialog__body">
          <header className="modal-dialog__header">
            <h2 className="modal-dialog__title" id={titleId}>
              {title}
            </h2>
          </header>
          <div className="modal-dialog__content">{children}</div>
          <footer className="modal-dialog__footer">
            {footer}
            <button
              className="modal-dialog__button"
              onClick={onClose}
              type="button"
              data-tip={closeLabel}
            >
              <ButtonFace icon="dismiss" label={closeLabel} />
            </button>
          </footer>
        </div>
      ) : null}
    </dialog>
  );
}
