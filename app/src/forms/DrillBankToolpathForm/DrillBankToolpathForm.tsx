import {
  lazy,
  Suspense,
  useCallback,
  useId,
  useMemo,
  useState,
  type FormEvent,
  type JSX,
} from 'react';

import type { DrillBankParams } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { lengthText } from '../../lib/formFields';
import { formatLength, unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { ProjectField } from '../ProjectField/ProjectField';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import type { DrillField } from '../DrillToolpathForm/model';
import {
  bankSummary,
  convertDrillBankUnits,
  drillBankParams,
  drillBankStateFrom,
  initialDrillBankState,
  validateDrillBank,
  type DrillBankField,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './DrillBankToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { cuttingDepths, peckPicture } from '../HelperPicture/pictures/toolpaths';

// The template dialog loads the first time Edit Drill Bank opens it.
const DrillBankTemplateDialog = lazy(async () => ({
  default: (await import('../DrillBankTemplateDialog/DrillBankTemplateDialog'))
    .DrillBankTemplateDialog,
}));

const text = strings.toolpaths;
const bank = text.drillBank;

const RETRACTS: ReadonlyArray<Choice<'full' | 'partial'>> = [
  { value: 'full', label: text.drill.fullRetract },
  { value: 'partial', label: text.drill.partialRetract },
];

export function DrillBankToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(
    initialDrillBankState,
    convertDrillBankUnits,
  );
  const load = useCallback(
    (params: DrillBankParams, name: string) =>
      setState(drillBankStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('drillBank', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const [editing, setEditing] = useState(false);
  const errors = useMemo(() => validateDrillBank(state, units), [state, units]);
  const nameId = useId();
  const unit = unitSuffix(units);
  const summary = bankSummary(state.template);
  const drill = state.drill;

  const set = (field: DrillBankField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const setDrill = (field: DrillField, value: string) =>
    setState((current) => ({ ...current, drill: { ...current.drill, [field]: value } }));
  const errorFor = (field: DrillBankField | DrillField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = drillBankParams(state, units, form.tool?.passDepth ?? 0);
    if (params !== null) {
      void form.calculate(params, state.name);
    }
  };

  return (
    <form className="drill-bank" onSubmit={onSubmit}>
      <p className="drill-bank__hint">
        {form.editingName === null ? bank.hint : text.editing(form.editingName)}
      </p>
      {!form.hasVectors ? <p className="drill-bank__notice">{text.needsSelection}</p> : null}
      <fieldset className="drill-bank__group">
        <legend className="drill-bank__legend">{text.depths}</legend>
        <HelperRow picture={<HelperPicture picture={cuttingDepths} />}>
          <NumberField
            error={errorFor('startDepth')}
            label={text.startDepth}
            onChange={(v) => setDrill('startDepth', v)}
            unit={unit}
            value={drill.startDepth}
          />
          <NumberField
            error={errorFor('cutDepth')}
            label={text.cutDepth}
            onChange={(v) => setDrill('cutDepth', v)}
            unit={unit}
            value={drill.cutDepth}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="drill-bank__group">
        <legend className="drill-bank__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="drill" tool={form.tool} units={units} />
      </fieldset>
      <fieldset className="drill-bank__group">
        <legend className="drill-bank__legend">{bank.bank}</legend>
        <p className="drill-bank__readout">
          {bank.summary(
            summary.drills,
            summary.grid,
            `${formatLength(state.template.pitch, units)} ${unit}`,
          )}
        </p>
        <button
          className="drill-bank__button"
          onClick={() => setEditing(true)}
          type="button"
          data-tip={bank.edit}
        >
          <ButtonFace icon="itemEdit" label={bank.edit} />
        </button>
        <NumberField
          error={errorFor('pitchTolerance')}
          label={bank.pitchTolerance}
          onChange={(v) => set('pitchTolerance', v)}
          unit={unit}
          value={state.pitchTolerance}
        />
        <NumberField
          error={errorFor('diameterTolerance')}
          label={bank.diameterTolerance}
          onChange={(v) => set('diameterTolerance', v)}
          unit={unit}
          value={state.diameterTolerance}
        />
      </fieldset>
      <fieldset className="drill-bank__group" aria-label={text.drill.peck}>
        <HelperRow picture={<HelperPicture picture={peckPicture(drill.peck)} />}>
          <CheckField
            checked={drill.peck}
            label={text.drill.peck}
            onChange={(peck) =>
              setState((current) => ({ ...current, drill: { ...current.drill, peck } }))
            }
          />
          <p className="drill-bank__readout">
            {text.drill.peckDepth}{' '}
            {form.tool === null
              ? text.drill.noTool
              : `${lengthText(form.tool.passDepth, units)} ${unit}`}
          </p>
          <ChoiceField
            disabled={!drill.peck}
            label={text.drill.retract}
            onChange={(retract) =>
              setState((current) => ({
                ...current,
                drill: { ...current.drill, fullRetract: retract === 'full' },
              }))
            }
            options={RETRACTS}
            value={drill.fullRetract ? 'full' : 'partial'}
          />
          <NumberField
            disabled={!drill.peck}
            error={errorFor('retractGap')}
            label={text.drill.retractGap}
            onChange={(v) => setDrill('retractGap', v)}
            unit={unit}
            value={drill.retractGap}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="drill-bank__group" aria-label={text.drill.useDwell}>
        <CheckField
          checked={drill.useDwell}
          label={text.drill.useDwell}
          onChange={(useDwell) =>
            setState((current) => ({ ...current, drill: { ...current.drill, useDwell } }))
          }
        />
        <NumberField
          disabled={!drill.useDwell}
          error={errorFor('dwell')}
          label={text.drill.dwell}
          onChange={(v) => setDrill('dwell', v)}
          unit={text.drill.seconds}
          value={drill.dwell}
        />
      </fieldset>
      <SafeZReadout />
      <ProjectField checked={form.project} onChange={form.setProject} />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="drill-bank__name" htmlFor={nameId}>
        {text.name}
        <input
          className="drill-bank__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={bank.title}
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
      {editing ? (
        <Suspense fallback={null}>
          <DrillBankTemplateDialog
            onApply={(template) => setState((current) => ({ ...current, template }))}
            onClose={() => setEditing(false)}
            template={state.template}
            units={units}
          />
        </Suspense>
      ) : null}
    </form>
  );
}
