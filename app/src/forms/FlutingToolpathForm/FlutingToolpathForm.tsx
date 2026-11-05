import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { FlutingParams, FlutingRamp } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useUiStore } from '../../state/uiStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { ProjectField } from '../ProjectField/ProjectField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { CheckField } from '../CheckField/CheckField';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import {
  convertFlutingUnits,
  flutingParams,
  flutingStateFrom,
  initialFlutingState,
  usesRampSize,
  validateFluting,
  type FluteType,
  type RampBy,
} from './model';

import './FlutingToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { flutePicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;

const RAMPS: ReadonlyArray<Choice<FlutingRamp>> = [
  { value: 'linear', label: text.fluting.linear },
  { value: 'smooth', label: text.fluting.smooth },
];

const TYPES: ReadonlyArray<Choice<FluteType>> = [
  { value: 'complete', label: text.fluting.complete },
  { value: 'start', label: text.fluting.atStart },
  { value: 'startEnd', label: text.fluting.atStartAndEnd },
];

const RAMP_BY: ReadonlyArray<Choice<RampBy>> = [
  { value: 'length', label: text.fluting.byLength },
  { value: 'percent', label: text.fluting.byPercent },
];

export function FlutingToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(
    initialFlutingState,
    convertFlutingUnits,
  );
  const load = useCallback(
    (params: FlutingParams, name: string) =>
      setState(flutingStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('fluting', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateFluting(state, units), [state, units]);
  const nameId = useId();
  const unit = unitSuffix(units);
  const sized = usesRampSize(state);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = flutingParams(state, units);
    if (params !== null) {
      void form.calculate(params, state.name);
    }
  };

  return (
    <form className="fluting-toolpath" onSubmit={onSubmit}>
      <p className="fluting-toolpath__hint">
        {form.editingName === null ? text.fluting.hint : text.editing(form.editingName)}
      </p>
      {!form.hasVectors ? <p className="fluting-toolpath__notice">{text.needsSelection}</p> : null}
      <fieldset className="fluting-toolpath__group">
        <legend className="fluting-toolpath__legend">{text.depths}</legend>
        <NumberField
          error={showErrors ? errors.startDepth : undefined}
          label={text.startDepth}
          onChange={(startDepth) => setState((current) => ({ ...current, startDepth }))}
          unit={unit}
          value={state.startDepth}
        />
        <NumberField
          error={showErrors ? errors.depth : undefined}
          label={text.fluting.depth}
          onChange={(depth) => setState((current) => ({ ...current, depth }))}
          unit={unit}
          value={state.depth}
        />
      </fieldset>
      <fieldset className="fluting-toolpath__group">
        <legend className="fluting-toolpath__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="ballNose" tool={form.tool} units={units} />
      </fieldset>
      <fieldset className="fluting-toolpath__group">
        <legend className="fluting-toolpath__legend">{text.fluting.fluteType}</legend>
        <HelperRow picture={<HelperPicture picture={flutePicture(state.fluteType)} />}>
          <ChoiceField
            label={text.fluting.fluteType}
            onChange={(fluteType) => setState((current) => ({ ...current, fluteType }))}
            options={TYPES}
            value={state.fluteType}
          />
        </HelperRow>
        <ChoiceField
          disabled={!sized}
          label={text.fluting.rampBy}
          onChange={(rampBy) => setState((current) => ({ ...current, rampBy }))}
          options={RAMP_BY}
          value={state.rampBy}
        />
        <NumberField
          disabled={!sized || state.rampBy !== 'length'}
          error={showErrors ? errors.rampLength : undefined}
          label={text.fluting.rampLength}
          onChange={(rampLength) => setState((current) => ({ ...current, rampLength }))}
          unit={unit}
          value={state.rampLength}
        />
        <NumberField
          disabled={!sized || state.rampBy !== 'percent'}
          error={showErrors ? errors.rampPercent : undefined}
          label={text.fluting.rampPercent}
          onChange={(rampPercent) => setState((current) => ({ ...current, rampPercent }))}
          unit="%"
          value={state.rampPercent}
        />
      </fieldset>
      <fieldset className="fluting-toolpath__group">
        <legend className="fluting-toolpath__legend">{text.fluting.rampType}</legend>
        <ChoiceField
          label={text.fluting.rampType}
          onChange={(ramp) => setState((current) => ({ ...current, ramp }))}
          options={RAMPS}
          value={state.ramp}
        />
      </fieldset>
      <CheckField
        checked={state.selectionOrder}
        label={strings.toolpaths.selectionOrder}
        onChange={(selectionOrder) => setState((current) => ({ ...current, selectionOrder }))}
      />
      <SafeZReadout />
      <ProjectField checked={form.project} onChange={form.setProject} />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="fluting-toolpath__name" htmlFor={nameId}>
        {text.name}
        <input
          className="fluting-toolpath__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.fluting.title}
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
