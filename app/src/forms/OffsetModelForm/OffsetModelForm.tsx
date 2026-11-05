import { useEffect, useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/api';
import { runModelAction } from '../../lib/runModelAction';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertOffsetModelUnits,
  initialOffsetModelState,
  offsetArgs,
  validateOffsetModel,
  type OffsetPreview,
} from './model';

import './OffsetModelForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { offsetModel } from '../HelperPicture/pictures/modeling';

const text = strings.model.refine.offset;
const common = strings.model.refine;

async function revert(preview: OffsetPreview | null): Promise<void> {
  if (preview !== null) {
    await runModelAction(() => api.model.refine.revertPreview(preview.revision));
  }
}

export function OffsetModelForm(): JSX.Element {
  const hasModel = useModelStore((store) => store.components.some((c) => c.visible));
  const lastError = useDocumentStore((store) => store.lastError);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialOffsetModelState, convertOffsetModelUnits);
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateOffsetModel(state, units), [state, units]);
  // The previews made so far, one undo step; null once kept or taken back.
  const previewRef = useRef<OffsetPreview | null>(null);
  const [previews, setPreviews] = useState(0);

  // Leaving the form any other way than OK takes the previews back.
  useEffect(
    () => () => {
      const pending = previewRef.current;
      previewRef.current = null;
      void revert(pending);
    },
    [],
  );

  const runPreview = async (): Promise<boolean> => {
    setShowErrors(true);
    const args = offsetArgs(state, units, previewRef.current);
    if (args === null) {
      return false;
    }
    const reply = await runModelAction(() => api.model.refine.offset(args));
    if (reply === null) {
      return false;
    }
    previewRef.current = { revision: reply.revision, id: reply.id };
    useModelStore.getState().select(reply.id);
    setPreviews((count) => count + 1);
    return true;
  };

  const ok = async () => {
    if (previewRef.current === null && !(await runPreview())) {
      return;
    }
    previewRef.current = null;
    closeForm();
  };

  const cancel = () => {
    const pending = previewRef.current;
    previewRef.current = null;
    void revert(pending).then(closeForm);
  };

  return (
    <form className="offset-model" onSubmit={(event) => event.preventDefault()}>
      <p className="offset-model__hint">{text.hint}</p>
      {hasModel ? null : <p className="offset-model__notice">{common.needsModel}</p>}
      <fieldset className="offset-model__group">
        <legend className="offset-model__legend">{common.settings}</legend>
        <HelperRow picture={<HelperPicture picture={offsetModel} />}>
          <NumberField
            error={showErrors ? errors.distance : undefined}
            label={text.distance}
            onChange={(distance) => setState((current) => ({ ...current, distance }))}
            onEnter={() => void runPreview()}
            unit={unitSuffix(units)}
            value={state.distance}
          />
          <CheckField
            checked={state.clipToZero}
            label={text.clip}
            onChange={(clipToZero) => setState((current) => ({ ...current, clipToZero }))}
          />
          <p className="offset-model__hint">{text.clipHint}</p>
          <p className="offset-model__hint">
            {previews === 0 ? text.previewHint : text.previewed(previews)}
          </p>
        </HelperRow>
      </fieldset>
      {lastError === null ? null : (
        <p className="offset-model__notice" role="alert">
          {lastError}
        </p>
      )}
      <FormButtons
        onClose={cancel}
        onPrimary={() => void ok()}
        onSecondary={() => void runPreview()}
        primaryWhy={hasModel ? undefined : common.needsModel}
        primaryLabel={text.ok}
        secondaryLabel={text.preview}
      />
    </form>
  );
}
