import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { DrillParams } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { lengthText } from '../../lib/formFields';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { ProjectField } from '../ProjectField/ProjectField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import {
  convertDrillUnits,
  drillParams,
  drillStateFrom,
  initialDrillState,
  validateDrill,
  type DrillField,
} from './model';

import './DrillToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { cuttingDepths, peckPicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;

const RETRACTS: ReadonlyArray<Choice<'full' | 'partial'>> = [
  { value: 'full', label: text.drill.fullRetract },
  { value: 'partial', label: text.drill.partialRetract },
];

export function DrillToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(initialDrillState, convertDrillUnits);
  const load = useCallback(
    (params: DrillParams, name: string) => setState(drillStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('drill', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateDrill(state, units), [state, units]);
  const nameId = useId();
  const unit = unitSuffix(units);

  const set = (field: DrillField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: DrillField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = drillParams(state, units, form.tool?.passDepth ?? 0);
    if (params !== null) {
      void form.calculate(params, state.name);
    }
  };

  return (
    <form className="drill-toolpath" onSubmit={onSubmit}>
      <p className="drill-toolpath__hint">
        {form.editingName === null ? text.drill.hint : text.editing(form.editingName)}
      </p>
      {!form.hasVectors ? <p className="drill-toolpath__notice">{text.needsSelection}</p> : null}
      <fieldset className="drill-toolpath__group">
        <legend className="drill-toolpath__legend">{text.depths}</legend>
        <HelperRow picture={<HelperPicture picture={cuttingDepths} />}>
          <NumberField
            error={errorFor('startDepth')}
            label={text.startDepth}
            onChange={(v) => set('startDepth', v)}
            unit={unit}
            value={state.startDepth}
          />
          <NumberField
            error={errorFor('cutDepth')}
            label={text.cutDepth}
            onChange={(v) => set('cutDepth', v)}
            unit={unit}
            value={state.cutDepth}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="drill-toolpath__group">
        <legend className="drill-toolpath__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="drill" tool={form.tool} units={units} />
      </fieldset>
      <fieldset className="drill-toolpath__group" aria-label={text.drill.peck}>
        <HelperRow picture={<HelperPicture picture={peckPicture(state.peck)} />}>
          <CheckField
            checked={state.peck}
            label={text.drill.peck}
            onChange={(peck) => setState((current) => ({ ...current, peck }))}
          />
          <p
            className={
              state.peck
                ? 'drill-toolpath__readout'
                : 'drill-toolpath__readout drill-toolpath__readout--off'
            }
          >
            {text.drill.peckDepth}{' '}
            {form.tool === null
              ? text.drill.noTool
              : `${lengthText(form.tool.passDepth, units)} ${unit}`}
          </p>
          <ChoiceField
            disabled={!state.peck}
            label={text.drill.retract}
            onChange={(retract) =>
              setState((current) => ({ ...current, fullRetract: retract === 'full' }))
            }
            options={RETRACTS}
            value={state.fullRetract ? 'full' : 'partial'}
          />
          <NumberField
            disabled={!state.peck}
            error={errorFor('retractGap')}
            label={text.drill.retractGap}
            onChange={(v) => set('retractGap', v)}
            unit={unit}
            value={state.retractGap}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="drill-toolpath__group" aria-label={text.drill.useDwell}>
        <CheckField
          checked={state.useDwell}
          label={text.drill.useDwell}
          onChange={(useDwell) => setState((current) => ({ ...current, useDwell }))}
        />
        <NumberField
          disabled={!state.useDwell}
          error={errorFor('dwell')}
          label={text.drill.dwell}
          onChange={(v) => set('dwell', v)}
          unit={text.drill.seconds}
          value={state.dwell}
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
      <label className="drill-toolpath__name" htmlFor={nameId}>
        {text.name}
        <input
          className="drill-toolpath__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.drill.title}
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
