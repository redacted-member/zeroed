import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { ChamferParams, ChamferSlope, ProfileSide } from '../../bridge/cam';
import { lengthText } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useUiStore } from '../../state/uiStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { DIRECTIONS } from '../ProfileToolpathForm/ProfileToolpathForm';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { CheckField } from '../CheckField/CheckField';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import {
  chamferAngle,
  chamferMaxDepth,
  chamferParams,
  chamferSizes,
  chamferStateFrom,
  chamferToolWarning,
  convertChamferUnits,
  editChamferSize,
  initialChamferState,
  isVShaped,
  validateChamfer,
  type ChamferBy,
  type ChamferField,
} from './model';

import './ChamferToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { chamfer } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;

const SIDES: ReadonlyArray<Choice<ProfileSide>> = [
  { value: 'inside', label: text.chamfer.inside },
  { value: 'outside', label: text.chamfer.outside },
];

/** Toolpaths made before the current layout may be cut on the vector. */
const SIDES_WITH_ON: ReadonlyArray<Choice<ProfileSide>> = [
  ...SIDES,
  { value: 'on', label: text.profile.on },
];

const SLOPES: ReadonlyArray<Choice<ChamferSlope>> = [
  { value: 'down', label: text.chamfer.slopeDown },
  { value: 'up', label: text.chamfer.slopeUp },
];

export function ChamferToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(
    initialChamferState,
    convertChamferUnits,
  );
  const load = useCallback(
    (params: ChamferParams, name: string) =>
      setState(chamferStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('chamfer', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateChamfer(state, units), [state, units]);
  const nameId = useId();
  const unit = unitSuffix(units);
  const toolWarning = chamferToolWarning(form.tool);
  const vShaped = form.tool !== null && isVShaped(form.tool);
  const angle = chamferAngle(state, form.tool);
  const maxDepth = chamferMaxDepth(state, units, form.tool);
  const sizes = chamferSizes(state, units, form.tool);
  const editSize = (field: ChamferBy, value: string) =>
    setState((current) => editChamferSize(current, field, value, units, form.tool));

  const set = (field: ChamferField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: ChamferField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = chamferParams(state, units);
    if (params !== null) {
      void form.calculate(params, state.name);
    }
  };

  return (
    <form className="chamfer-toolpath" onSubmit={onSubmit}>
      <p className="chamfer-toolpath__hint">
        {form.editingName === null ? text.chamfer.hint : text.editing(form.editingName)}
      </p>
      {!form.hasVectors ? <p className="chamfer-toolpath__notice">{text.needsSelection}</p> : null}
      <fieldset className="chamfer-toolpath__group">
        <legend className="chamfer-toolpath__legend">{text.depths}</legend>
        <NumberField
          error={errorFor('startDepth')}
          label={text.startDepth}
          onChange={(v) => set('startDepth', v)}
          unit={unit}
          value={state.startDepth}
        />
      </fieldset>
      <fieldset className="chamfer-toolpath__group">
        <legend className="chamfer-toolpath__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="vBit" tool={form.tool} units={units} />
        {toolWarning === undefined ? null : (
          <p className="chamfer-toolpath__notice" role="status">
            {toolWarning}
          </p>
        )}
      </fieldset>
      <fieldset className="chamfer-toolpath__group">
        <legend className="chamfer-toolpath__legend">{text.chamfer.dimensions}</legend>
        <HelperRow picture={<HelperPicture picture={chamfer} />}>
          <NumberField
            disabled={vShaped}
            error={errorFor('angle')}
            label={text.chamfer.angle}
            onChange={(v) => set('angle', v)}
            unit={strings.units.degrees}
            value={vShaped && angle !== null ? String(angle) : state.angle}
          />
          <NumberField
            error={errorFor('width')}
            label={text.chamfer.width}
            onChange={(v) => editSize('width', v)}
            unit={unit}
            value={sizes.width}
          />
          <NumberField
            error={errorFor('depth')}
            label={text.chamfer.depth}
            onChange={(v) => editSize('depth', v)}
            unit={unit}
            value={sizes.depth}
          />
        </HelperRow>
        <NumberField
          disabled={state.side === 'on'}
          error={errorFor('overcut')}
          label={text.chamfer.overcut}
          onChange={(v) => set('overcut', v)}
          unit={unit}
          value={state.overcut}
        />
        <NumberField
          disabled
          label={text.chamfer.maxDepth}
          onChange={() => undefined}
          placeholder={text.chamfer.maxDepthUnknown}
          unit={unit}
          value={maxDepth === null ? '' : lengthText(maxDepth, units)}
        />
      </fieldset>
      <fieldset className="chamfer-toolpath__group" aria-label={text.chamfer.side}>
        <ChoiceField
          label={text.chamfer.side}
          onChange={(side) => setState((current) => ({ ...current, side }))}
          options={state.side === 'on' ? SIDES_WITH_ON : SIDES}
          value={state.side}
        />
        <ChoiceField
          disabled={state.side === 'on'}
          label={text.chamfer.slope}
          onChange={(slope) => setState((current) => ({ ...current, slope }))}
          options={SLOPES}
          value={state.slope}
        />
      </fieldset>
      <ChoiceField
        label={text.cutDirection}
        onChange={(direction) => setState((current) => ({ ...current, direction }))}
        options={DIRECTIONS}
        value={state.direction}
      />
      <CheckField
        checked={state.selectionOrder}
        label={strings.toolpaths.selectionOrder}
        onChange={(selectionOrder) => setState((current) => ({ ...current, selectionOrder }))}
      />
      <SafeZReadout />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="chamfer-toolpath__name" htmlFor={nameId}>
        {text.name}
        <input
          className="chamfer-toolpath__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.chamfer.title}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={form.calculateWhy}
        primaryLabel={text.calculate}
        submit
      />
    </form>
  );
}
