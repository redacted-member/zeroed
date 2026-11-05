import { useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/api';
import { runModelAction } from '../../lib/runModelAction';
import { strings } from '../../lib/formStrings';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { SliderField } from '../SliderField/SliderField';
import {
  MAX_SLIDER_ANGLE,
  MIN_SLIDER_ANGLE,
  draftAngle,
  initialAddDraftState,
  sliderAngle,
  validateAddDraft,
  type AddDraftState,
} from './model';

import './AddDraftForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { draft } from '../HelperPicture/pictures/modeling';

const text = strings.model.refine.draft;
const common = strings.model.refine;

export function AddDraftForm(): JSX.Element {
  const hasModel = useModelStore((store) => store.components.some((c) => c.visible));
  const closeForm = useUiStore((store) => store.closeForm);
  const [state, setState] = useState<AddDraftState>(initialAddDraftState);
  const [showErrors, setShowErrors] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const errors = useMemo(() => validateAddDraft(state), [state]);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const angle = draftAngle(state);
    if (angle === null) {
      return;
    }
    void runModelAction(() => api.model.refine.addDraft(angle)).then((reply) => {
      if (reply === null) {
        setFailure(useDocumentStore.getState().lastError);
        setStatus(null);
        return;
      }
      setFailure(null);
      useModelStore.getState().select(reply.id);
      const name = useModelStore.getState().components.find((c) => c.id === reply.id)?.name;
      setStatus(text.added(name ?? text.title));
    });
  };

  return (
    <form className="add-draft" onSubmit={onSubmit}>
      <p className="add-draft__hint">{text.hint}</p>
      {hasModel ? null : <p className="add-draft__notice">{common.needsModel}</p>}
      <fieldset className="add-draft__group">
        <legend className="add-draft__legend">{common.settings}</legend>
        <HelperRow picture={<HelperPicture picture={draft} />}>
          <SliderField
            label={text.angle}
            max={MAX_SLIDER_ANGLE}
            min={MIN_SLIDER_ANGLE}
            onChange={(angle) => setState({ angle: String(angle) })}
            step={0.5}
            unit={strings.units.degrees}
            value={sliderAngle(state)}
          />
          <NumberField
            error={showErrors ? errors.angle : undefined}
            label={text.angle}
            onChange={(angle) => setState({ angle })}
            unit={strings.units.degrees}
            value={state.angle}
          />
        </HelperRow>
      </fieldset>
      {status === null ? null : (
        <p className="add-draft__hint" role="status">
          {status}
        </p>
      )}
      {failure === null ? null : (
        <p className="add-draft__notice" role="alert">
          {failure}
        </p>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={hasModel ? undefined : common.needsModel}
        primaryLabel={common.apply}
        submit
      />
    </form>
  );
}
