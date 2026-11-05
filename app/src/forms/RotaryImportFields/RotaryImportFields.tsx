import { useState, type JSX } from 'react';

import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDocumentStore } from '../../state/documentStore';
import { useImportStore } from '../../state/importStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { NumberField } from '../NumberField/NumberField';
import {
  fitFromState,
  fitToBlank,
  originalSize,
  rotaryErrors,
  rotaryFitState,
  typeSize,
  withCylinder,
  withOffset,
  type RotaryFitState,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './RotaryImportFields.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { rotaryWrapPicture } from '../HelperPicture/pictures/modeling';

const text = strings.model.importModel.rotary;

const AXIS_MOVE: ReadonlyArray<Choice<'off' | 'on'>> = [
  { value: 'off', label: text.off },
  { value: 'on', label: text.on },
];

/** Rotation Axis Movement and Model Scaling of a full 3D model on a rotary
 * job; `units` is the unit the model's sizes show in. */
export function RotaryImportFields({ units }: { units: Units }): JSX.Element {
  const fit = useImportStore((store) => store.rotaryFit);
  const cylinder = useImportStore((store) => store.info?.cylinder ?? null);
  const fitWhy = cylinder === null ? strings.why.loading : undefined;
  const resize = useImportStore((store) => store.resizeMaterial);
  const job = useDocumentStore((store) => store.job);
  const axisMove = useImportStore((store) => store.axisMove);
  const [state, setState] = useState<RotaryFitState>(() => rotaryFitState(fit, cylinder, units));
  // A new cylinder from the kernel (a turn or a new size) fills the sizes.
  const [shown, setShown] = useState(cylinder);
  if (cylinder !== shown) {
    setShown(cylinder);
    if (cylinder !== null) {
      setState(withCylinder(state, cylinder, units));
    }
  }
  // Offsets set by panning in the 3D view fill the fields.
  const [shownOffset, setShownOffset] = useState(fit.offset);
  if (fit.offset !== shownOffset) {
    setShownOffset(fit.offset);
    setState(withOffset(state, fit.offset, units));
  }
  const errors = rotaryErrors(state, units);
  const suffix = unitSuffix(units);
  const store = useImportStore.getState;

  const commit = (next: RotaryFitState) => {
    const wanted = fitFromState(next, store().rotaryFit, cylinder, units);
    if (wanted !== null) {
      store().setRotaryFit(wanted);
      void store().refresh();
    }
  };

  const refit = (next: typeof fit) => {
    store().setRotaryFit(next);
    void store().refresh();
  };

  return (
    <>
      <fieldset className="rotary-import__group">
        <legend className="rotary-import__legend">{text.axisMovement}</legend>
        <p className="rotary-import__hint">{text.axisHint}</p>
        <HelperRow picture={<HelperPicture picture={rotaryWrapPicture(axisMove)} />}>
          <ChoiceField
            label={text.axisMovement}
            onChange={(on) => useImportStore.getState().setAxisMove(on === 'on')}
            options={AXIS_MOVE}
            value={axisMove ? 'on' : 'off'}
          />
        </HelperRow>
        <NumberField
          error={errors.side}
          label={text.side}
          onChange={(side) => setState({ ...state, side })}
          onEnter={() => commit(state)}
          unit={suffix}
          value={state.side}
        />
        <NumberField
          error={errors.up}
          label={text.up}
          onChange={(up) => setState({ ...state, up })}
          onEnter={() => commit(state)}
          unit={suffix}
          value={state.up}
        />
      </fieldset>
      <fieldset className="rotary-import__group">
        <legend className="rotary-import__legend">{text.scaling}</legend>
        <NumberField
          error={errors.length}
          label={text.length}
          onChange={(value) => setState(typeSize(state, 'length', value, cylinder, units))}
          onEnter={() => commit(state)}
          unit={suffix}
          value={state.length}
        />
        <NumberField
          error={errors.diameter}
          label={text.diameter}
          onChange={(value) => setState(typeSize(state, 'diameter', value, cylinder, units))}
          onEnter={() => commit(state)}
          unit={suffix}
          value={state.diameter}
        />
        <CheckField
          checked={state.lockRatio}
          label={text.lockRatio}
          onChange={(lockRatio) => setState({ ...state, lockRatio })}
        />
        <div className="rotary-import__row">
          <button
            className="rotary-import__button"
            disabled={fitWhy !== undefined}
            onClick={() =>
              cylinder !== null &&
              refit(
                fitToBlank(
                  fit,
                  cylinder,
                  { length: job.rotary.length ?? cylinder.length, diameter: job.rotary.diameter },
                  state.lockRatio,
                ),
              )
            }
            type="button"
            data-tip={whyTip(text.fitMaterial, fitWhy)}
          >
            <ButtonFace icon="setSize" label={text.fitMaterial} />
          </button>
          <button
            className="rotary-import__button"
            onClick={() => refit(originalSize(fit))}
            type="button"
            data-tip={text.originalSize}
          >
            <ButtonFace icon="formReset" label={text.originalSize} />
          </button>
        </div>
        <CheckField
          checked={resize}
          label={text.resizeMaterial}
          onChange={(on) => store().setResizeMaterial(on)}
        />
      </fieldset>
    </>
  );
}
