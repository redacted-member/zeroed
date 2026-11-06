import { useCallback, useEffect, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { PocketParams, StraightInlayParams, Tool } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { nextToolpathName } from '../../lib/toolpathNames';
import { depthValues } from '../../lib/toolpathFields';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useToolpathStore } from '../../state/toolpathStore';
import { rotaryRasterAngle } from '../../lib/rotary';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { DepthFields } from '../DepthFields/DepthFields';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { PassesRow } from '../PassesRow/PassesRow';
import {
  convertPocketUnits,
  initialPocketState,
  validatePocket,
  type PocketField,
  type ProfilePassChoice,
} from '../PocketToolpathForm/model';
import { DIRECTIONS } from '../ProfileToolpathForm/ProfileToolpathForm';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { SelectField } from '../SelectField/SelectField';
import { baseNameOf, KIND_TITLES } from '../StraightInlayToolpathForm/model';
import { ToolListField } from '../ToolListField/ToolListField';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import { inlayPocketParams, inlayPocketStateFrom } from './model';

import './InlayPocketForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { clearancePicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;

const STRATEGIES: ReadonlyArray<Choice<PocketParams['strategy']>> = [
  { value: 'offset', label: text.pocket.offset },
  { value: 'raster', label: text.pocket.raster },
];

const PROFILE_PASSES: ReadonlyArray<Choice<ProfilePassChoice>> = [
  { value: 'none', label: text.pocket.profileNone },
  { value: 'first', label: text.pocket.profileFirst },
  { value: 'last', label: text.pocket.profileLast },
];

/** Female Inlay - Pocket: the Pocket form, its Pocket Allowance growing the cavity. */
export function InlayPocketForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(initialPocketState, convertPocketUnits);
  const load = useCallback(
    (params: StraightInlayParams, name: string) =>
      setState(inlayPocketStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('straightInlay', load, {
    setName: () => {
      const names = useToolpathStore.getState().toolpaths.map((t) => t.name);
      const name = nextToolpathName(baseNameOf('femalePocket'), names);
      setState((current) => ({ ...current, name }));
    },
  });
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validatePocket(state, units), [state, units]);
  const depthMm = useMemo(() => depthValues(state.depths, units), [state.depths, units]);
  const nameId = useId();
  const unit = unitSuffix(units);
  const raster = state.strategy === 'raster';
  const alongBlank = useDocumentStore((documents) => rotaryRasterAngle(documents.job));
  useEffect(() => {
    if (alongBlank !== null) {
      setState((current) => ({
        ...current,
        strategy: 'raster',
        rasterAngle: String(alongBlank),
      }));
    }
  }, [alongBlank, setState]);
  const { tool: firstTool, setTool } = form;
  const tools = useMemo(
    () => (firstTool === null ? state.laterTools : [firstTool, ...state.laterTools]),
    [firstTool, state.laterTools],
  );
  const setTools = useCallback(
    (next: Tool[]) => {
      const [first, ...rest] = next;
      if (first !== undefined) {
        setTool(first);
      }
      setState((current) => ({ ...current, laterTools: rest }));
    },
    [setState, setTool],
  );

  const set = (field: PocketField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: PocketField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = inlayPocketParams(state, units);
    if (params !== null) {
      void form.calculate(params, state.name);
    }
  };

  return (
    <form className="inlay-pocket" onSubmit={onSubmit}>
      <h3 className="inlay-pocket__title">{KIND_TITLES.femalePocket}</h3>
      {form.editingName === null ? null : (
        <p className="inlay-pocket__hint">{text.editing(form.editingName)}</p>
      )}
      {!form.hasVectors ? <p className="inlay-pocket__notice">{text.needsSelection}</p> : null}
      <DepthFields
        errors={showErrors ? errors : {}}
        onChange={(depths) => setState((current) => ({ ...current, depths }))}
        state={state.depths}
        units={units}
      />
      <fieldset className="inlay-pocket__group">
        <legend className="inlay-pocket__legend">{text.tool}</legend>
        <ToolListField
          autoPick
          minimum={1}
          onChange={setTools}
          prefer="endMill"
          tools={tools}
          units={units}
        />
        <PassesRow
          cutDepth={depthMm?.cutDepth ?? null}
          onChange={(passes) => setState((current) => ({ ...current, passes }))}
          startDepth={depthMm?.startDepth ?? null}
          state={state.passes}
          toolPassDepth={form.tool?.passDepth ?? null}
          units={units}
        />
      </fieldset>
      <HelperRow picture={<HelperPicture picture={clearancePicture(state.strategy)} />}>
        <fieldset className="inlay-pocket__group" aria-label={text.pocket.clearing}>
          <ChoiceField
            disabled={alongBlank !== null}
            label={text.pocket.clearing}
            onChange={(strategy) => setState((current) => ({ ...current, strategy }))}
            options={STRATEGIES}
            value={state.strategy}
          />
          <ChoiceField
            label={text.cutDirection}
            onChange={(direction) => setState((current) => ({ ...current, direction }))}
            options={DIRECTIONS}
            value={state.direction}
          />
          <NumberField
            disabled={!raster || alongBlank !== null}
            error={errorFor('rasterAngle')}
            label={text.pocket.rasterAngle}
            onChange={(v) => set('rasterAngle', v)}
            unit={strings.units.degrees}
            value={state.rasterAngle}
          />
          <SelectField
            disabled={!raster}
            label={text.pocket.profilePass}
            onChange={(profilePass) => setState((current) => ({ ...current, profilePass }))}
            options={PROFILE_PASSES}
            value={state.profilePass}
          />
        </fieldset>
      </HelperRow>
      <fieldset className="inlay-pocket__group" aria-label={text.useRamps}>
        <CheckField
          checked={state.ramp}
          label={text.useRamps}
          onChange={(ramp) => setState((current) => ({ ...current, ramp }))}
        />
        <NumberField
          disabled={!state.ramp}
          error={errorFor('rampDistance')}
          label={text.rampDistance}
          onChange={(v) => set('rampDistance', v)}
          unit={unit}
          value={state.rampDistance}
        />
      </fieldset>
      <NumberField
        error={errorFor('allowance')}
        label={text.pocket.allowance}
        onChange={(v) => set('allowance', v)}
        unit={unit}
        value={state.allowance}
      />
      <CheckField
        checked={state.selectionOrder}
        label={text.selectionOrder}
        onChange={(selectionOrder) => setState((current) => ({ ...current, selectionOrder }))}
      />
      <SafeZReadout />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="inlay-pocket__name" htmlFor={nameId}>
        {text.name}
        <input
          className="inlay-pocket__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
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
