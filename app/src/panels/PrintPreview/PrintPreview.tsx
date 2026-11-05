import type { JSX } from 'react';

import { printView } from '../../lib/printView';
import { strings } from '../../lib/strings';
import { useDialogStore } from '../../state/dialogStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';

import './PrintPreview.scss';

const text = strings.printing;

/** File > Print Preview: the page as it prints, with a Print button. */
export function PrintPreview(): JSX.Element {
  const open = useDialogStore((state) => state.open === 'printPreview');
  const image = useDialogStore((state) => state.printImage);
  const close = useDialogStore((state) => state.close);
  return (
    <ModalDialog
      className="print-preview"
      closeLabel={text.close}
      footer={
        <button
          className="modal-dialog__button"
          onClick={() => {
            close();
            printView();
          }}
          type="button"
          data-tip={text.print}
        >
          <ButtonFace icon="print" label={text.print} />
        </button>
      }
      onClose={close}
      open={open}
      title={text.previewTitle}
      titleId="print-preview-title"
    >
      <div className="print-preview__page">
        {image === null ? null : (
          <img alt={text.imageAlt} className="print-preview__image" src={image} />
        )}
      </div>
    </ModalDialog>
  );
}
