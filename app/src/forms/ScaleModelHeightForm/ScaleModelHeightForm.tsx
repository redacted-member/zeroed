import { useMemo, useState, type JSX } from 'react';

import { api } from '../../bridge/api';
import { isDoubleSided } from '../../lib/sides';
import { strings } from '../../lib/formStrings';
import { formatLength, unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { usePreviewSession } from '../../lib/usePreviewSession';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { SliderField } from '../SliderField/SliderField';
import {
  MAX_PERCENT,
  MIN_PERCENT,
  convertScaleModelHeightUnits,
  exactArgs,
  initialScaleModelHeightState,
  percentArgs,
  percentOf,
  validateScaleModelHeight,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './ScaleModelHeightForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { scaleHeight } from '../HelperPicture/pictures/modeling';

const text = strings.model.refine.scaleHeight;
const common = strings.model.refine;

export function ScaleModelHeightForm(): JSX.Element {
  const modelTop = useModelStore((store) => store.composite?.max ?? null);
  const lastError = useDocumentStore((store) => store.lastError);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(
    (u) => initialScaleModelHeightState(u, modelTop),
    convertScaleModelHeightUnits,
  );
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateScaleModelHeight(state, units), [state, units]);
  const session = usePreviewSession('discard');
  const unit = unitSuffix(units);
  const noModel = modelTop === null || modelTop <= 0;
  // A double sided job can scale the other side's model with this one.
  const doubleSided = useDocumentStore((store) => isDoubleSided(store.job));
  const [bothSides, setBothSides] = useState(false);
  const both = doubleSided && bothSides;

  const previewPercent = (percent: number) => {
    if (percent === 100) {
      void session.discard();
      return;
    }
    void session.preview((replace) =>
      api.model.refine.scaleHeight(percentArgs(percent, replace, both)),
    );
  };

  const setExact = () => {
    setShowErrors(true);
    void session.preview(async (replace) => {
      const args = exactArgs(state, units, replace, both);
      if (args === null) {
        throw new Error(errors.height ?? strings.form.errorNumber);
      }
      const reply = await api.model.refine.scaleHeight(args);
      setState((current) => ({ ...current, percent: percentOf(reply.factor) }));
      return reply;
    });
  };

  const apply = async () => {
    await session.keep();
    const top = useModelStore.getState().composite?.max ?? null;
    setState((current) => ({
      percent: 100,
      height: top === null ? current.height : formatLength(top, units),
    }));
  };

  const heightWhy = noModel ? text.noModel : undefined;
  return (
    <form className="scale-model-height" onSubmit={(event) => event.preventDefault()}>
      <p className="scale-model-height__hint">{text.hint}</p>
      <p className={noModel ? 'scale-model-height__notice' : 'scale-model-height__target'}>
        {noModel ? text.noModel : text.current(`${formatLength(modelTop ?? 0, units)} ${unit}`)}
      </p>
      <CheckField
        checked={doubleSided && bothSides}
        disabled={!doubleSided}
        label={text.bothSides}
        onChange={setBothSides}
      />
      <fieldset className="scale-model-height__group">
        <legend className="scale-model-height__legend">{common.settings}</legend>
        <HelperRow picture={<HelperPicture picture={scaleHeight} />}>
          <SliderField
            disabled={noModel}
            label={text.scale}
            max={MAX_PERCENT}
            min={MIN_PERCENT}
            onChange={(percent) => setState((current) => ({ ...current, percent }))}
            onCommit={previewPercent}
            unit={common.percent}
            value={state.percent}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="scale-model-height__group">
        <legend className="scale-model-height__legend">{text.exact}</legend>
        <NumberField
          disabled={noModel}
          error={showErrors ? errors.height : undefined}
          label={text.height}
          onChange={(height) => setState((current) => ({ ...current, height }))}
          onEnter={setExact}
          unit={unit}
          value={state.height}
        />
        <button
          className="scale-model-height__set"
          disabled={heightWhy !== undefined}
          onClick={setExact}
          type="button"
          data-tip={whyTip(text.setHeight, heightWhy)}
        >
          <ButtonFace icon="setValue" label={text.setHeight} />
        </button>
      </fieldset>
      {lastError === null ? null : (
        <p className="scale-model-height__notice" role="alert">
          {lastError}
        </p>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => void apply()}
        primaryWhy={noModel ? text.noModel : undefined}
        primaryLabel={common.apply}
      />
    </form>
  );
}
