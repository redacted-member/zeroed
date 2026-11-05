import { useId, useState, type JSX } from 'react';

import { fileName, importVectorPage } from '../../lib/fileCommands';
import { strings } from '../../lib/formStrings';
import { useDialogStore } from '../../state/dialogStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import { pageValue, stepPage } from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './PdfPageDialog.scss';

const text = strings.pdfPage;

/** Import of a PDF with several pages: which page to bring in. */
export function PdfPageDialog(): JSX.Element {
  const request = useDialogStore((state) => (state.open === 'pdfPage' ? state.pdfPage : null));
  const close = useDialogStore((state) => state.close);
  const [page, setPage] = useState('1');
  const [shown, setShown] = useState(request);
  const fieldId = useId();
  const hintId = useId();
  const errorId = useId();

  // Each new file starts at its first page.
  if (shown !== request) {
    setShown(request);
    setPage('1');
  }

  const pages = request?.pages ?? 1;
  const chosen = pageValue(page, pages);
  const importWhy = chosen === null ? strings.why.choosePage : undefined;

  const submit = () => {
    if (request === null || chosen === null) {
      return;
    }
    close();
    void importVectorPage(request.path, chosen, request.asJob === true);
  };

  return (
    <ModalDialog
      className="pdf-page-dialog"
      closeLabel={text.cancel}
      footer={
        <button
          className="modal-dialog__button"
          disabled={importWhy !== undefined}
          form={fieldId + '-form'}
          type="submit"
          data-tip={whyTip(text.import, importWhy)}
        >
          <ButtonFace icon="importVectors" label={text.import} />
        </button>
      }
      onClose={close}
      open={request !== null}
      title={text.title}
      titleId="pdf-page-dialog-title"
    >
      {request === null ? null : (
        <form
          className="pdf-page-dialog__body"
          id={fieldId + '-form'}
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <p className="pdf-page-dialog__text" id={hintId}>
            {text.intro(fileName(request.path), request.pages)}
          </p>
          <div className="pdf-page-dialog__row">
            <label className="pdf-page-dialog__label" htmlFor={fieldId}>
              {text.page}
            </label>
            <input
              aria-describedby={chosen === null ? `${hintId} ${errorId}` : hintId}
              aria-invalid={chosen === null}
              className="pdf-page-dialog__field"
              id={fieldId}
              inputMode="numeric"
              onChange={(event) => setPage(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
                  event.preventDefault();
                  setPage(stepPage(page, pages, event.key === 'ArrowUp' ? 1 : -1));
                }
              }}
              value={page}
            />
            <span className="pdf-page-dialog__of">{text.of(request.pages)}</span>
          </div>
          {chosen === null ? (
            <p className="pdf-page-dialog__error" id={errorId} role="alert">
              {text.range(request.pages)}
            </p>
          ) : null}
        </form>
      )}
    </ModalDialog>
  );
}
