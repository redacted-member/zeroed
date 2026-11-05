import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { VGrooveParams } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { CheckNumberField } from '../CheckNumberField/CheckNumberField';
import { ClearanceToolsField } from '../ClearanceToolsField/ClearanceToolsField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { ProjectField } from '../ProjectField/ProjectField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import {
  convertVGrooveUnits,
  initialVGrooveState,
  validateVGroove,
  vgrooveParams,
  vgrooveStateFrom,
} from './model';

import './VGrooveToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { vgrooveDepthPicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;

export function VGrooveToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(
    initialVGrooveState,
    convertVGrooveUnits,
  );
  const load = useCallback(
    (params: VGrooveParams, name: string) =>
      setState(vgrooveStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('vgroove', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateVGroove(state, units), [state, units]);
  const nameId = useId();
  const unit = unitSuffix(units);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = vgrooveParams(state, units);
    if (params !== null) {
      void form.calculate(params, state.name);
    }
  };

  return (
    <form className="vgroove-toolpath" onSubmit={onSubmit}>
      <p className="vgroove-toolpath__hint">
        {form.editingName === null ? text.vgroove.hint : text.editing(form.editingName)}
      </p>
      {!form.hasVectors ? <p className="vgroove-toolpath__notice">{text.needsSelection}</p> : null}
      <fieldset className="vgroove-toolpath__group">
        <legend className="vgroove-toolpath__legend">{text.depths}</legend>
        <HelperRow picture={<HelperPicture picture={vgrooveDepthPicture(state.useFlatDepth)} />}>
          <NumberField
            error={showErrors ? errors.startDepth : undefined}
            label={text.startDepth}
            onChange={(startDepth) => setState((current) => ({ ...current, startDepth }))}
            unit={unit}
            value={state.startDepth}
          />
          <CheckNumberField
            checked={state.useFlatDepth}
            error={showErrors ? errors.flatDepth : undefined}
            label={text.vgroove.flatDepth}
            offText={text.noTool}
            onChange={(flatDepth) => setState((current) => ({ ...current, flatDepth }))}
            onCheck={(useFlatDepth) => setState((current) => ({ ...current, useFlatDepth }))}
            unit={unit}
            value={state.flatDepth}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="vgroove-toolpath__group">
        <legend className="vgroove-toolpath__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="vBit" tool={form.tool} units={units} />
      </fieldset>
      <ClearanceToolsField
        enabled={state.useClearance}
        entries={state.clearanceTools}
        onChange={(clearanceTools) => setState((current) => ({ ...current, clearanceTools }))}
        onEnable={(useClearance) => setState((current) => ({ ...current, useClearance }))}
        sharpen
        showErrors={showErrors}
        units={units}
      />
      <fieldset className="vgroove-toolpath__group" aria-label={text.vgroove.vectorOrder}>
        <CheckField
          checked={state.vectorStart}
          label={text.vgroove.vectorStart}
          onChange={(vectorStart) => setState((current) => ({ ...current, vectorStart }))}
        />
        <CheckField
          checked={state.selectionOrder}
          label={text.selectionOrder}
          onChange={(selectionOrder) => setState((current) => ({ ...current, selectionOrder }))}
        />
      </fieldset>
      <SafeZReadout />
      <ProjectField checked={form.project} onChange={form.setProject} />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="vgroove-toolpath__name" htmlFor={nameId}>
        {text.name}
        <input
          className="vgroove-toolpath__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.vgroove.title}
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
