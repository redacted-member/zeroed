import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { LaserSketchParams, SketchFill } from '../../bridge/sketch';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { ProjectField } from '../ProjectField/ProjectField';
import type { SketchSourceState } from '../SketchSourceFields/model';
import { SketchSourceFields } from '../SketchSourceFields/SketchSourceFields';
import { useSketchSource } from '../SketchSourceFields/useSketchSource';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import {
  convertLaserSketchUnits,
  initialLaserSketchState,
  laserSketchParams,
  laserSketchStateFrom,
  validateLaserSketch,
  type LaserSketchField,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './LaserSketchForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { laserSketchPicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;
const sketch = text.sketch;
const laser = text.laser;

const FILLS: ReadonlyArray<Choice<SketchFill>> = [
  { value: 'together', label: sketch.fillTogether },
  { value: 'regionally', label: sketch.fillRegionally },
  { value: 'regionallyWithinVectors', label: sketch.fillWithin },
];

export function LaserSketchForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(
    initialLaserSketchState,
    convertLaserSketchUnits,
  );
  const load = useCallback(
    (params: LaserSketchParams, name: string) =>
      setState(laserSketchStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('laserSketch', load, {
    requireSelection: false,
    ownSelector: true,
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const setSource = useCallback(
    (update: SketchSourceState | ((current: SketchSourceState) => SketchSourceState)) =>
      setState((current) => ({
        ...current,
        source: typeof update === 'function' ? update(current.source) : update,
      })),
    [setState],
  );
  const handle = useSketchSource(state.source, setSource, units);
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(
    () => validateLaserSketch(state, units, handle.boundary.length),
    [state, units, handle.boundary.length],
  );
  const shown = showErrors ? errors : {};
  const nameId = useId();
  const unit = unitSuffix(units);
  const isLaser = form.tool?.toolType.kind === 'laserTool';

  const set = (field: LaserSketchField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = laserSketchParams(state, units, handle.boundary.length);
    if (params !== null && isLaser) {
      void form.calculate(params, state.name, { ids: handle.ids });
    }
  };

  return (
    <form className="laser-sketch" onSubmit={onSubmit}>
      <p className="laser-sketch__hint">
        {form.editingName === null ? sketch.laserHint : text.editing(form.editingName)}
      </p>
      <fieldset className="laser-sketch__group">
        <legend className="laser-sketch__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="laserTool" tool={form.tool} units={units} />
        {form.tool !== null && !isLaser ? (
          <p className="laser-sketch__notice">{laser.notLaser}</p>
        ) : null}
      </fieldset>
      <SketchSourceFields
        errors={shown}
        handle={handle}
        onChange={setSource}
        part="selection"
        state={state.source}
        units={units}
      />
      <fieldset className="laser-sketch__group">
        <legend className="laser-sketch__legend">{laser.power}</legend>
        <NumberField
          error={shown.power}
          label={laser.power}
          onChange={(v) => set('power', v)}
          unit={laser.percent}
          value={state.power}
        />
        <p className="laser-sketch__hint">{laser.toolSetting}</p>
        <NumberField
          error={shown.overscan}
          label={laser.overscan}
          onChange={(v) => set('overscan', v)}
          unit={unit}
          value={state.overscan}
        />
      </fieldset>
      <SketchSourceFields
        errors={shown}
        handle={handle}
        onChange={setSource}
        part="limits"
        state={state.source}
        units={units}
      />
      <fieldset className="laser-sketch__group">
        <legend className="laser-sketch__legend">{sketch.fillOptions}</legend>
        <HelperRow picture={<HelperPicture picture={laserSketchPicture(state.crossHatch)} />}>
          <NumberField
            error={shown.stepover}
            label={sketch.stepover}
            onChange={(v) => set('stepover', v)}
            unit={unit}
            value={state.stepover}
          />
          <NumberField
            error={shown.angle}
            label={sketch.hatchAngle}
            onChange={(v) => set('angle', v)}
            unit={strings.units.degrees}
            value={state.angle}
          />
          <ChoiceField
            label={sketch.fillOptions}
            onChange={(fill) => setState((current) => ({ ...current, fill }))}
            options={FILLS}
            value={state.fill}
          />
          <CheckField
            checked={state.crossHatch}
            label={sketch.crossHatch}
            onChange={(crossHatch) => setState((current) => ({ ...current, crossHatch }))}
          />
        </HelperRow>
      </fieldset>
      <SafeZReadout />
      <ProjectField checked={form.project} onChange={form.setProject} />
      <VectorSelectionField
        filter={state.selector}
        onChange={(selector) => setState((current) => ({ ...current, selector }))}
      />
      <label className="laser-sketch__name" htmlFor={nameId}>
        {text.name}
        <input
          className="laser-sketch__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={sketch.laserTitle}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy([form.tool === null, text.needsTool], [!isLaser, laser.notLaser])}
        primaryLabel={text.calculate}
        submit
      />
    </form>
  );
}
