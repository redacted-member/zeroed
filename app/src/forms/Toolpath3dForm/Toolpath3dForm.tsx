import { useCallback, useEffect, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type {
  Tool,
  Boundary3d,
  CutDirection,
  Finish3dParams,
  Finish3dStrategy,
  Rough3dOrder,
  Rough3dParams,
  Rough3dProfile,
  Rough3dStrategy,
} from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { numberValue } from '../../lib/formFields';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { calculateWhy, useToolpathForm } from '../../lib/useToolpathForm';
import { useModelStore } from '../../state/modelStore';
import { rotaryRasterAngle } from '../../lib/rotary';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { SliderBoxField } from '../SliderBoxField/SliderBoxField';
import { ToolListField } from '../ToolListField/ToolListField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import {
  angleFor,
  convert3dUnits,
  finish3dParams,
  form3dOnRotary,
  initial3dState,
  isZLevel,
  minimumDetailRange,
  needsLevel,
  rasterDirection,
  rough3dParams,
  state3dFrom,
  usesAngle,
  usesProfile,
  validate3d,
  type Field3d,
  type Form3dState,
  type Kind3d,
  type RasterDirection,
} from './model';

import './Toolpath3dForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { boundary3dPicture, strategy3dPicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths.threeD;

const BOUNDARIES: ReadonlyArray<Choice<Boundary3d>> = [
  { value: 'model', label: text.model },
  { value: 'material', label: text.material },
  { value: 'selection', label: text.selection },
  { value: 'level', label: text.levelBoundary },
];

const FINISH_STRATEGIES: ReadonlyArray<Choice<Finish3dStrategy>> = [
  { value: 'offset', label: text.offset },
  { value: 'raster', label: text.raster },
];

const ROUGH_STRATEGIES: ReadonlyArray<Choice<Rough3dStrategy>> = [
  { value: 'zLevelRaster', label: text.zLevel },
  { value: 'raster3d', label: text.raster3d },
];

const PROFILES: ReadonlyArray<Choice<Rough3dProfile>> = [
  { value: 'first', label: text.profileFirst },
  { value: 'last', label: text.profileLast },
  { value: 'none', label: text.profileNone },
];

const ORDERS: ReadonlyArray<Choice<Rough3dOrder>> = [
  { value: 'levelByLevel', label: text.levelByLevel },
  { value: 'depthFirst', label: text.depthFirst },
];

const DIRECTIONS: ReadonlyArray<Choice<RasterDirection>> = [
  { value: 'x', label: text.alongX },
  { value: 'y', label: text.alongY },
];

const CUT_DIRECTIONS: ReadonlyArray<Choice<CutDirection>> = [
  { value: 'climb', label: text.climb },
  { value: 'conventional', label: text.conventional },
];

function Toolpath3dForm({ kind }: { kind: Kind3d }): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const hasModel = useModelStore((store) => store.components.length > 0);
  const levels = useModelStore((store) => store.levels);
  const { state, setState, units, unitsRef } = useFormState(
    (u) => initial3dState(kind, u),
    convert3dUnits,
  );
  const load = useCallback(
    (params: Finish3dParams | Rough3dParams, name: string) =>
      setState(state3dFrom(kind, params, name, unitsRef.current)),
    [kind, setState, unitsRef],
  );
  const form = useToolpathForm(kind, load, {
    requireSelection: false,
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validate3d(kind, state, units), [kind, state, units]);
  const nameId = useId();
  const levelId = useId();
  const unit = unitSuffix(units);
  const needsVectors = state.boundary === 'selection' && !form.hasVectors;
  const missingLevel = needsLevel(state);
  const rough = kind === 'rough3d';
  const { tool: firstTool, setTool } = form;
  const tools = useMemo(
    () => (firstTool === null ? state.restTools : [firstTool, ...state.restTools]),
    [firstTool, state.restTools],
  );
  const setTools = useCallback(
    (next: Tool[]) => {
      const [first, ...rest] = next;
      if (first !== undefined) {
        setTool(first);
      }
      setState((current) => ({ ...current, restTools: rest }));
    },
    [setState, setTool],
  );
  const detailRange = minimumDetailRange(units);

  const set = (field: Field3d, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const patch = (next: Partial<Form3dState>) => setState((current) => ({ ...current, ...next }));
  const alongBlank = useDocumentStore((documents) => rotaryRasterAngle(documents.job));
  const editing = useUiStore((store) => store.editingToolpath !== null);
  useEffect(() => {
    if (alongBlank !== null) {
      setState((current) => form3dOnRotary(current, alongBlank, editing));
    }
  }, [alongBlank, editing, setState]);
  const errorFor = (field: Field3d) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = rough ? rough3dParams(state, units) : finish3dParams(state, units);
    if (params !== null && !needsVectors) {
      void form.calculate(params as Rough3dParams, state.name);
    }
  };

  const zLevel = isZLevel(state);

  const roughStrategy = (
    <fieldset className="toolpath-3d__group">
      <legend className="toolpath-3d__legend">{text.roughStrategy}</legend>
      <HelperRow picture={<HelperPicture picture={strategy3dPicture(state.roughStrategy)} />}>
        <ChoiceField
          label={text.strategy}
          onChange={(roughStrategy) => patch({ roughStrategy })}
          options={ROUGH_STRATEGIES}
          value={state.roughStrategy}
        />
      </HelperRow>
      <ChoiceField
        disabled={!usesProfile(state)}
        label={text.profile}
        onChange={(profile) => patch({ profile })}
        options={PROFILES}
        value={usesProfile(state) ? state.profile : 'none'}
      />
      <ChoiceField
        disabled={!zLevel}
        label={text.order}
        onChange={(order) => patch({ order })}
        options={ORDERS}
        value={state.order}
      />
      <ChoiceField
        disabled={!usesAngle(kind, state) || alongBlank !== null}
        label={text.rasterDirection}
        onChange={(direction) => patch({ angle: angleFor(direction) })}
        options={DIRECTIONS}
        value={rasterDirection(state)}
      />
      <CheckField
        checked={state.reverseStep}
        disabled={state.roughStrategy !== 'zLevelRaster'}
        label={text.reverseStep}
        onChange={(reverseStep) => patch({ reverseStep })}
      />
      <CheckField
        checked={!zLevel && state.avoidMachined}
        disabled={zLevel}
        label={text.avoidMachined}
        onChange={(avoidMachined) => patch({ avoidMachined })}
      />
    </fieldset>
  );

  const finishStrategy = (
    <fieldset className="toolpath-3d__group">
      <legend className="toolpath-3d__legend">{text.areaStrategy}</legend>
      <HelperRow picture={<HelperPicture picture={strategy3dPicture(state.finishStrategy)} />}>
        <ChoiceField
          disabled={alongBlank !== null}
          label={text.strategy}
          onChange={(finishStrategy) => patch({ finishStrategy })}
          options={FINISH_STRATEGIES}
          value={state.finishStrategy}
        />
      </HelperRow>
      <ChoiceField
        disabled={state.finishStrategy !== 'offset'}
        label={text.cutDirection}
        onChange={(direction) => patch({ direction })}
        options={CUT_DIRECTIONS}
        value={state.direction}
      />
      <NumberField
        disabled={!usesAngle(kind, state) || alongBlank !== null}
        error={errorFor('angle')}
        label={text.rasterAngle}
        onChange={(v) => set('angle', v)}
        unit={strings.units.degrees}
        value={state.angle}
      />
      <NumberField
        disabled={state.finishStrategy !== 'offset'}
        error={errorFor('stepoverRetract')}
        label={text.stepoverRetract}
        onChange={(v) => set('stepoverRetract', v)}
        unit={unit}
        value={state.stepoverRetract}
      />
    </fieldset>
  );

  return (
    <form className="toolpath-3d" onSubmit={onSubmit}>
      {alongBlank !== null ? (
        <p className="toolpath-3d__hint">{strings.toolpaths.pocket.rotaryClearing}</p>
      ) : null}
      <p className="toolpath-3d__hint">
        {form.editingName !== null
          ? strings.toolpaths.editing(form.editingName)
          : rough
            ? text.roughHint
            : text.finishHint}
      </p>
      {hasModel ? null : <p className="toolpath-3d__notice">{text.noModel}</p>}
      {rough ? (
        <fieldset className="toolpath-3d__group">
          <legend className="toolpath-3d__legend">{strings.toolpaths.tool}</legend>
          <ToolPicker onChange={form.setTool} prefer="endMill" tool={form.tool} units={units} />
        </fieldset>
      ) : (
        <fieldset className="toolpath-3d__group" aria-label={strings.toolpaths.toolList.label}>
          <ToolListField
            autoPick
            minimum={1}
            onChange={setTools}
            prefer="ballNose"
            tools={tools}
            units={units}
          />
        </fieldset>
      )}
      <fieldset className="toolpath-3d__group">
        <legend className="toolpath-3d__legend">{text.boundary}</legend>
        <HelperRow picture={<HelperPicture picture={boundary3dPicture(state.boundary)} />}>
          <ChoiceField
            label={text.boundary}
            onChange={(boundary) => patch({ boundary })}
            options={BOUNDARIES}
            value={state.boundary}
          />
        </HelperRow>
        <label className="toolpath-3d__name" htmlFor={levelId}>
          {text.level}
          <select
            className="toolpath-3d__input"
            disabled={state.boundary !== 'level'}
            id={levelId}
            onChange={(event) => patch({ level: Number(event.target.value) })}
            value={state.level}
          >
            <option value={0}>{text.chooseLevel}</option>
            {levels.map((level) => (
              <option key={level.id} value={level.id}>
                {level.name}
              </option>
            ))}
          </select>
        </label>
        {needsVectors ? <p className="toolpath-3d__notice">{text.needsVectors}</p> : null}
        {missingLevel ? <p className="toolpath-3d__notice">{text.needsLevel}</p> : null}
        <NumberField
          error={errorFor('boundaryOffset')}
          label={text.boundaryOffset}
          onChange={(v) => set('boundaryOffset', v)}
          unit={unit}
          value={state.boundaryOffset}
        />
      </fieldset>
      {rough ? (
        <fieldset className="toolpath-3d__group" aria-label={text.allowance}>
          <NumberField
            error={errorFor('allowance')}
            label={text.allowance}
            onChange={(v) => set('allowance', v)}
            unit={unit}
            value={state.allowance}
          />
        </fieldset>
      ) : null}
      {rough ? null : (
        <fieldset className="toolpath-3d__group">
          <legend className="toolpath-3d__legend">{text.restMachining}</legend>
          <SliderBoxField
            disabled={state.restTools.length === 0}
            endLabel={text.fine}
            label={text.minimumDetail}
            max={detailRange.max}
            min={detailRange.min}
            onChange={(value) => set('minimumDetail', String(value))}
            reverse
            startLabel={text.coarse}
            step={detailRange.step}
            unit={unit}
            value={numberValue(state.minimumDetail) ?? detailRange.min}
          />
          <NumberField
            disabled={state.restTools.length === 0}
            error={errorFor('restOffset')}
            label={text.restOffset}
            onChange={(v) => set('restOffset', v)}
            unit={unit}
            value={state.restOffset}
          />
        </fieldset>
      )}
      {rough ? roughStrategy : finishStrategy}
      {rough ? (
        <fieldset className="toolpath-3d__group">
          <legend className="toolpath-3d__legend">{text.ramps}</legend>
          <CheckField
            checked={zLevel && state.ramp}
            disabled={!zLevel}
            label={text.useRamps}
            onChange={(ramp) => patch({ ramp })}
          />
          <NumberField
            disabled={!zLevel || !state.ramp}
            error={errorFor('rampDistance')}
            label={text.rampDistance}
            onChange={(v) => set('rampDistance', v)}
            unit={unit}
            value={state.rampDistance}
          />
        </fieldset>
      ) : null}
      <SafeZReadout />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="toolpath-3d__name" htmlFor={nameId}>
        {strings.toolpaths.name}
        <input
          className="toolpath-3d__input"
          id={nameId}
          onChange={(event) => patch({ name: event.target.value })}
          placeholder={rough ? text.roughTitle : text.finishTitle}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={
          !hasModel
            ? text.noModel
            : needsVectors
              ? text.needsVectors
              : missingLevel
                ? text.needsLevel
                : calculateWhy(true, form.tool !== null)
        }
        primaryLabel={strings.toolpaths.calculate}
        submit
      />
    </form>
  );
}

export function Rough3dToolpathForm(): JSX.Element {
  return <Toolpath3dForm kind="rough3d" />;
}

export function Finish3dToolpathForm(): JSX.Element {
  return <Toolpath3dForm kind="finish3d" />;
}
