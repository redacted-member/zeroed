import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import { formatLength } from '../../lib/units';
import type { LaserCutFillParams } from '../../bridge/cam';
import type { VectorFilter } from '../../bridge/sketch';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { lengthValue } from '../../lib/formFields';
import { startFrom, type Remembered } from '../../lib/rememberedForm';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { ImmediateOutput } from '../ImmediateOutput/ImmediateOutput';
import { NumberField } from '../NumberField/NumberField';
import { ProjectField } from '../ProjectField/ProjectField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { isAutomatic, withSelector } from '../VectorSelectionField/model';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import {
  convertLaserCutFillUnits,
  fills,
  takesAllowance,
  type LaserStrategy,
  initialLaserCutFillState,
  laserCutFillParams,
  laserCutFillStateFrom,
  laserPasses,
  validateLaserCutFill,
  type LaserCutFillField,
  type LaserCutFillState,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './LaserCutFillForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { laserCutPicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;
const laser = text.laser;

const FILL_WAYS: ReadonlyArray<Choice<'together' | 'regions'>> = [
  { value: 'together', label: laser.fillTogether },
  { value: 'regions', label: laser.fillRegionally },
];

const STRATEGIES: ReadonlyArray<Choice<LaserStrategy>> = [
  { value: 'outside', label: laser.cutOutside },
  { value: 'inside', label: laser.cutInside },
  { value: 'on', label: laser.cutOn },
  { value: 'hatch', label: laser.hatchFill },
];

/** The fields as last calculated, for the next new form. */
let lastCutFill: Remembered<LaserCutFillState> | null = null;

function initialState(units: Parameters<typeof initialLaserCutFillState>[0]): LaserCutFillState {
  return startFrom(
    lastCutFill,
    units,
    initialLaserCutFillState,
    convertLaserCutFillUnits,
    (state) => ({ ...state, name: '' }),
  );
}

export function LaserCutFillForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(initialState, convertLaserCutFillUnits);
  const [selector, setSelector] = useState<VectorFilter | null>(null);
  const automatic = isAutomatic(selector);
  const load = useCallback(
    (params: LaserCutFillParams, name: string) => {
      setState(laserCutFillStateFrom(params, name, unitsRef.current));
      setSelector(params.selector ?? null);
    },
    [setState, unitsRef],
  );
  const form = useToolpathForm('laserCutFill', load, {
    ownSelector: true,
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  // The toolpath this form calculated last: Immediate Output saves it, and
  // Calculate again recalculates it, as the form stays open.
  const [lastId, setLastId] = useState<number | null>(() => useUiStore.getState().editingToolpath);
  const errors = useMemo(() => validateLaserCutFill(state, units), [state, units]);
  const nameId = useId();
  const unit = unitSuffix(units);
  const filling = fills(state);
  const isLaser = form.tool?.toolType.kind === 'laserTool';
  const passes = laserPasses(lengthValue(state.cutDepth, units), form.tool?.passDepth ?? 0);
  const calculated = (ok: boolean) => {
    if (ok) {
      lastCutFill = { state, units };
    }
    const id = useToolpathStore.getState().selectedId;
    if (ok && id !== null) {
      setLastId(id);
      useUiStore.setState({ editingToolpath: id });
    }
  };

  const set = (field: LaserCutFillField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: LaserCutFillField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = laserCutFillParams(state, units);
    if (params === null || !isLaser) {
      return;
    }
    if (automatic && selector !== null) {
      // The kernel picks the vectors again from the filter on every calculation.
      void api.cam
        .selectVectors(selector)
        .then((picked) =>
          form.calculate(withSelector(params, selector), state.name, {
            ids: picked.ids,
            keepOpen: true,
          }),
        )
        .then(calculated);
    } else {
      void form.calculate(params, state.name, { keepOpen: true }).then(calculated);
    }
  };

  return (
    <form className="laser-cut-fill" onSubmit={onSubmit}>
      <p className="laser-cut-fill__hint">
        {form.editingName === null ? laser.cutFillHint : text.editing(form.editingName)}
      </p>
      {form.selectedCount === 0 && !automatic ? (
        <p className="laser-cut-fill__notice">{text.needsSelection}</p>
      ) : null}
      <fieldset className="laser-cut-fill__group">
        <legend className="laser-cut-fill__legend">{laser.toolSection}</legend>
        <ToolPicker onChange={form.setTool} prefer="laserTool" tool={form.tool} units={units} />
        {form.tool !== null && !isLaser ? (
          <p className="laser-cut-fill__notice">{laser.notLaser}</p>
        ) : null}
        <NumberField
          error={errorFor('power')}
          label={laser.power}
          onChange={(v) => set('power', v)}
          unit={laser.percent}
          value={state.power}
        />
        <NumberField
          error={errorFor('speed')}
          label={laser.speed}
          onChange={(v) => set('speed', v)}
          unit={laser.perMinute(unit)}
          value={state.speed}
        />
        <p className="laser-cut-fill__hint">{laser.toolSetting}</p>
        <NumberField
          error={errorFor('cutDepth')}
          label={laser.cutDepth}
          onChange={(v) => set('cutDepth', v)}
          unit={unit}
          value={state.cutDepth}
        />
        <p className="laser-cut-fill__readout">
          {laser.passesCount}: {passes}
          {form.tool === null || passes === 1
            ? ''
            : ` (${formatLength(form.tool.passDepth, units)} ${unit})`}
        </p>
        <NumberField
          error={errorFor('repeats')}
          label={laser.repeats}
          onChange={(v) => set('repeats', v)}
          step={1}
          value={state.repeats}
        />
        <NumberField
          disabled={!filling}
          error={errorFor('overscan')}
          label={laser.overscan}
          onChange={(v) => set('overscan', v)}
          unit={laser.percent}
          value={state.overscan}
        />
      </fieldset>
      <fieldset className="laser-cut-fill__group">
        <legend className="laser-cut-fill__legend">{laser.strategy}</legend>
        <HelperRow picture={<HelperPicture picture={laserCutPicture(state.strategy)} />}>
          <ChoiceField
            label={laser.strategy}
            onChange={(strategy) => setState((current) => ({ ...current, strategy }))}
            options={STRATEGIES}
            value={state.strategy}
          />
        </HelperRow>
        <NumberField
          disabled={!takesAllowance(state)}
          error={errorFor('allowance')}
          label={laser.allowance}
          onChange={(v) => set('allowance', v)}
          unit={unit}
          value={state.allowance}
        />
        <NumberField
          disabled={!filling}
          error={errorFor('lineInterval')}
          label={laser.lineInterval}
          onChange={(v) => set('lineInterval', v)}
          unit={unit}
          value={state.lineInterval}
        />
        <NumberField
          disabled={!filling}
          error={errorFor('angle')}
          label={laser.angle}
          onChange={(v) => set('angle', v)}
          unit={strings.units.degrees}
          value={state.angle}
        />
        <ChoiceField
          disabled={!filling}
          label={laser.fillWay}
          onChange={(way) =>
            setState((current) => ({ ...current, fillRegions: way === 'regions' }))
          }
          options={FILL_WAYS}
          value={state.fillRegions ? 'regions' : 'together'}
        />
        <CheckField
          checked={state.hatch === 'cross'}
          disabled={!filling}
          label={laser.cross}
          onChange={(cross) =>
            setState((current) => ({ ...current, hatch: cross ? 'cross' : 'lines' }))
          }
        />
        <CheckField
          checked={state.outline}
          disabled={!filling}
          label={laser.outline}
          onChange={(outline) => setState((current) => ({ ...current, outline }))}
        />
      </fieldset>
      <SafeZReadout />
      <ProjectField checked={form.project} onChange={form.setProject} />
      <VectorSelectionField filter={selector} onChange={setSelector} />
      <label className="laser-cut-fill__name" htmlFor={nameId}>
        {text.name}
        <input
          className="laser-cut-fill__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={laser.cutFillTitle}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [form.tool === null, text.needsTool],
          [!isLaser, laser.notLaser],
          [form.selectedCount === 0 && !automatic, text.needsSelection],
        )}
        primaryLabel={text.calculate}
        submit
      />
      <ImmediateOutput toolpathId={lastId} />
    </form>
  );
}
