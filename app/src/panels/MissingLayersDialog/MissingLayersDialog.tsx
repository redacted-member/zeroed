import type { JSX } from 'react';

import type { MissingLayersChoice } from '../../bridge/api';
import { strings } from '../../lib/formStrings';
import { useDialogStore } from '../../state/dialogStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';

import './MissingLayersDialog.scss';

const text = strings.toolpathShell.list;

const CHOICES: ReadonlyArray<{ value: MissingLayersChoice; label: string; hint: string }> = [
  { value: 'create', label: text.missingCreate, hint: text.missingCreateHint },
  { value: 'delete', label: text.missingDelete, hint: text.missingDeleteHint },
  { value: 'asIs', label: text.missingAsIs, hint: text.missingAsIsHint },
];

/** Loading a toolpath template that names layers the job lacks: what to do
 * with the toolpaths on them. */
export function MissingLayersDialog(): JSX.Element {
  const request = useDialogStore((state) =>
    state.open === 'missingLayers' ? state.missingLayers : null,
  );
  const close = useDialogStore((state) => state.close);

  const choose = (choice: MissingLayersChoice) => {
    if (request === null) {
      return;
    }
    close();
    void useToolpathStore.getState().loadTemplate(request.path, choice);
  };

  return (
    <ModalDialog
      className="missing-layers"
      closeLabel={text.cancel}
      onClose={close}
      open={request !== null}
      title={text.missingLayersTitle}
      titleId="missing-layers-title"
    >
      {request === null ? null : (
        <div className="missing-layers__body">
          <p className="missing-layers__text">{text.missingLayersIntro(request.layers.length)}</p>
          <ul className="missing-layers__layers">
            {request.layers.map((layer) => (
              <li className="missing-layers__layer" key={layer}>
                {layer}
              </li>
            ))}
          </ul>
          <div className="missing-layers__choices">
            {CHOICES.map((choice) => (
              <div className="missing-layers__choice" key={choice.value}>
                <button
                  className="modal-dialog__button"
                  onClick={() => choose(choice.value)}
                  type="button"
                  data-tip={choice.label}
                >
                  <ButtonFace icon="layers" label={choice.label} />
                </button>
                <p className="missing-layers__hint">{choice.hint}</p>
              </div>
            ))}
          </div>
        </div>
      )}
    </ModalDialog>
  );
}
