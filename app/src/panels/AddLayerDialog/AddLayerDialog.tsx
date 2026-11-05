import { useId, useState, type FormEvent, type JSX } from 'react';

import type { Layer } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { useDocumentStore } from '../../state/documentStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import { newLayerDraft, newLayerRequest, type NewLayerDraft } from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './AddLayerDialog.scss';

const text = strings.layersPanel.addDialog;

interface AddLayerDialogProps {
  layers: readonly Layer[];
  /** Inserts the new layer before this one; absent adds it last. */
  before?: number;
  onClose: () => void;
}

/** Add New Layer: name, colour, and whether the layer starts visible and active. */
export function AddLayerDialog({ layers, before, onClose }: AddLayerDialogProps): JSX.Element {
  const [draft, setDraft] = useState<NewLayerDraft>(() => newLayerDraft(layers));
  const addLayer = useDocumentStore((state) => state.addLayer);
  const id = useId();
  const request = newLayerRequest(draft, before);
  const okWhy = request === null ? strings.why.typeName : undefined;

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (request === null) {
      return;
    }
    void addLayer(request.name, request.colour, request.options).then(onClose);
  };

  return (
    <ModalDialog
      className="add-layer-dialog"
      closeLabel={strings.form.cancel}
      footer={
        <button
          className="modal-dialog__button"
          disabled={okWhy !== undefined}
          form={`${id}-form`}
          type="submit"
          data-tip={whyTip(text.ok, okWhy)}
        >
          <ButtonFace icon="formApply" label={text.ok} />
        </button>
      }
      onClose={onClose}
      open
      title={before === undefined ? text.title : text.insertTitle}
      titleId={`${id}-title`}
    >
      <form className="add-layer-dialog__form" id={`${id}-form`} onSubmit={onSubmit}>
        <label className="add-layer-dialog__label" htmlFor={`${id}-name`}>
          {text.name}
        </label>
        <input
          autoComplete="off"
          className="add-layer-dialog__input"
          id={`${id}-name`}
          onChange={(event) => setDraft({ ...draft, name: event.target.value })}
          required
          type="text"
          value={draft.name}
        />
        <label className="add-layer-dialog__label" htmlFor={`${id}-colour`}>
          {text.colour}
        </label>
        <input
          className="add-layer-dialog__colour"
          id={`${id}-colour`}
          onChange={(event) => setDraft({ ...draft, colour: event.target.value })}
          type="color"
          value={draft.colour}
        />
        <label className="add-layer-dialog__check">
          <input
            checked={draft.visible}
            onChange={(event) => setDraft({ ...draft, visible: event.target.checked })}
            type="checkbox"
          />
          {text.visible}
        </label>
        <label className="add-layer-dialog__check">
          <input
            checked={draft.active}
            onChange={(event) => setDraft({ ...draft, active: event.target.checked })}
            type="checkbox"
          />
          {text.active}
        </label>
      </form>
    </ModalDialog>
  );
}
