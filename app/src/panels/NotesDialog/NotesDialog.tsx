import { useEffect, useId, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import { dialogsStrings } from '../../lib/strings/dialogs';
import { useDialogStore } from '../../state/dialogStore';
import { useDocumentStore } from '../../state/documentStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';
import { whyStrings } from '../../lib/strings/why';

import './NotesDialog.scss';

const text = dialogsStrings.notesDialog;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Edit > Notes: free text kept in the job. */
export function NotesDialog(): JSX.Element {
  const open = useDialogStore((state) => state.open === 'notes');
  const close = useDialogStore((state) => state.close);
  const [notes, setNotes] = useState<string | null>(null);
  const okWhy = notes === null ? whyStrings.why.notesLoading : undefined;
  const [failure, setFailure] = useState<string | null>(null);
  const fieldId = useId();
  const hintId = useId();

  useEffect(() => {
    if (!open) {
      return;
    }
    let live = true;
    void api.doc
      .getNotes()
      .then((reply) => {
        if (live) {
          setNotes(reply.notes);
          setFailure(null);
        }
      })
      .catch((error: unknown) => {
        if (live) {
          setFailure(message(error));
        }
      });
    return () => {
      live = false;
      setNotes(null);
    };
  }, [open]);

  const save = async () => {
    if (notes === null) {
      return;
    }
    try {
      await api.doc.setNotes(notes);
      await useDocumentStore.getState().refresh();
      close();
    } catch (error) {
      setFailure(message(error));
    }
  };

  return (
    <ModalDialog
      className="notes-dialog"
      closeLabel={text.cancel}
      footer={
        <button
          className="modal-dialog__button"
          disabled={okWhy !== undefined}
          onClick={() => void save()}
          type="button"
          data-tip={whyTip(text.ok, okWhy)}
        >
          <ButtonFace icon="formApply" label={text.ok} />
        </button>
      }
      onClose={close}
      open={open}
      title={text.title}
      titleId="notes-dialog-title"
    >
      <div className="notes-dialog__body">
        <label className="notes-dialog__label" htmlFor={fieldId}>
          {text.label}
        </label>
        <textarea
          aria-describedby={hintId}
          className="notes-dialog__text"
          disabled={notes === null}
          id={fieldId}
          onChange={(event) => setNotes(event.target.value)}
          rows={14}
          spellCheck
          value={notes ?? ''}
        />
        <p className="notes-dialog__hint" id={hintId}>
          {text.hint}
        </p>
        {failure === null ? null : (
          <p className="notes-dialog__error" role="alert">
            {failure}
          </p>
        )}
      </div>
    </ModalDialog>
  );
}
