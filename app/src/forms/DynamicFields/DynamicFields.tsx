import { useEffect, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { useModelStore, type AnchorPurpose } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { NumberField } from '../NumberField/NumberField';
import { SliderField } from '../SliderField/SliderField';
import { numberValue } from '../../lib/formFields';
import { validateDynamic, withAnchors, type DynamicState } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './DynamicFields.scss';

const text = strings.model.dynamic;

interface DynamicFieldsProps {
  state: DynamicState;
  onChange: (next: DynamicState) => void;
  showErrors?: boolean;
}

/** Fade and Tilt: a check box each, Set Anchor (two clicks in the 3D view)
 * and the amount or angle, typed or by slider. */
export function DynamicFields({ state, onChange, showErrors }: DynamicFieldsProps): JSX.Element {
  const pick = useModelStore((store) => store.anchorPick);
  const errors = showErrors === true ? validateDynamic(state) : {};

  // Two clicks in: they become the anchors of whichever asked for them.
  useEffect(() => {
    if (pick !== null && pick.points.length >= 2) {
      onChange(withAnchors(state, pick.purpose, pick.points));
      useModelStore.getState().cancelAnchorPick();
    }
  }, [pick, state, onChange]);

  // A form closing mid pick stops the picking.
  useEffect(() => () => useModelStore.getState().cancelAnchorPick(), []);

  const setAnchor = (purpose: AnchorPurpose) => {
    const ui = useUiStore.getState();
    if (ui.viewMode === '2d') {
      ui.setViewMode('split');
    }
    useModelStore.getState().startAnchorPick(purpose);
  };

  const status = (purpose: AnchorPurpose, set: boolean) => {
    if (pick !== null && pick.purpose === purpose && pick.points.length < 2) {
      return pick.points.length === 0 ? text.clickFirst : text.clickSecond;
    }
    return set ? text.anchorsSet : text.noAnchors;
  };

  const fadeSet = state.fadeFrom !== null && state.fadeTo !== null;
  const tiltSet = state.tiltPivot !== null && state.tiltToward !== null;
  const fadeValue = numberValue(state.fadePercent) ?? 0;
  const tiltValue = numberValue(state.tiltAngle) ?? 0;

  const fadeWhy = state.fadeOn ? undefined : strings.why.tickFirst(text.fade);
  const tiltWhy = state.tiltOn ? undefined : strings.why.tickFirst(text.tilt);
  return (
    <div className="dynamic-fields">
      <fieldset className="dynamic-fields__group">
        <legend className="dynamic-fields__legend">
          <CheckField
            checked={state.fadeOn}
            label={text.fade}
            onChange={(fadeOn) => onChange({ ...state, fadeOn })}
          />
        </legend>
        <div className="dynamic-fields__row">
          <button
            className="dynamic-fields__button"
            disabled={fadeWhy !== undefined}
            onClick={() => setAnchor('fade')}
            type="button"
            data-tip={whyTip(text.setAnchor, fadeWhy)}
          >
            <ButtonFace icon="origin" label={text.setAnchor} />
          </button>
          <p aria-live="polite" className="dynamic-fields__status">
            {status('fade', fadeSet)}
          </p>
        </div>
        <NumberField
          disabled={!state.fadeOn}
          error={errors.fadePercent}
          label={text.fadeAmount}
          onChange={(fadePercent) => onChange({ ...state, fadePercent })}
          unit="%"
          value={state.fadePercent}
        />
        <SliderField
          disabled={!state.fadeOn}
          label={text.fadeSlider}
          max={100}
          min={0}
          onChange={(v) => onChange({ ...state, fadePercent: String(v) })}
          unit="%"
          value={Math.min(100, Math.max(0, fadeValue))}
        />
      </fieldset>
      <fieldset className="dynamic-fields__group">
        <legend className="dynamic-fields__legend">
          <CheckField
            checked={state.tiltOn}
            label={text.tilt}
            onChange={(tiltOn) => onChange({ ...state, tiltOn })}
          />
        </legend>
        <div className="dynamic-fields__row">
          <button
            className="dynamic-fields__button"
            disabled={tiltWhy !== undefined}
            onClick={() => setAnchor('tilt')}
            type="button"
            data-tip={whyTip(text.setAnchor, tiltWhy)}
          >
            <ButtonFace icon="origin" label={text.setAnchor} />
          </button>
          <p aria-live="polite" className="dynamic-fields__status">
            {status('tilt', tiltSet)}
          </p>
        </div>
        <NumberField
          disabled={!state.tiltOn}
          error={errors.tiltAngle}
          label={text.tiltAngle}
          onChange={(tiltAngle) => onChange({ ...state, tiltAngle })}
          unit={strings.units.degrees}
          value={state.tiltAngle}
        />
        <SliderField
          disabled={!state.tiltOn}
          label={text.tiltSlider}
          max={45}
          min={-45}
          onChange={(v) => onChange({ ...state, tiltAngle: String(v) })}
          step={0.5}
          unit={strings.units.degrees}
          value={Math.min(45, Math.max(-45, tiltValue))}
        />
      </fieldset>
    </div>
  );
}
