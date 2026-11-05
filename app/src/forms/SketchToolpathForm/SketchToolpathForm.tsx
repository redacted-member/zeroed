import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { SketchCarveParams } from '../../bridge/sketch';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useUiStore } from '../../state/uiStore';
import { CheckNumberField } from '../CheckNumberField/CheckNumberField';
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
  convertSketchUnits,
  initialSketchState,
  isCarvingTool,
  SKETCH_TOOL_KINDS,
  sketchParams,
  sketchStateFrom,
  validateSketch,
} from './model';

import './SketchToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { vgrooveDepthPicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;
const sketch = text.sketch;

export function SketchToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(initialSketchState, convertSketchUnits);
  const load = useCallback(
    (params: SketchCarveParams, name: string) =>
      setState(sketchStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('sketchCarve', load, {
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
    () => validateSketch(state, units, handle.boundary.length),
    [state, units, handle.boundary.length],
  );
  const shown = showErrors ? errors : {};
  const nameId = useId();
  const unit = unitSuffix(units);
  const carving = isCarvingTool(form.tool);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = sketchParams(state, units, handle.boundary.length);
    if (params !== null && carving) {
      void form.calculate(params, state.name, { ids: handle.ids });
    }
  };

  return (
    <form className="sketch-toolpath" onSubmit={onSubmit}>
      <p className="sketch-toolpath__hint">
        {form.editingName === null ? sketch.hint : text.editing(form.editingName)}
      </p>
      <SketchSourceFields
        errors={shown}
        handle={handle}
        onChange={setSource}
        part="selection"
        state={state.source}
        units={units}
      />
      <fieldset className="sketch-toolpath__group">
        <legend className="sketch-toolpath__legend">{text.depths}</legend>
        <HelperRow picture={<HelperPicture picture={vgrooveDepthPicture(state.useFlatDepth)} />}>
          <NumberField
            error={shown.startDepth}
            label={text.startDepth}
            onChange={(startDepth) => setState((current) => ({ ...current, startDepth }))}
            unit={unit}
            value={state.startDepth}
          />
          <CheckNumberField
            checked={state.useFlatDepth}
            error={shown.flatDepth}
            label={sketch.flatDepth}
            offText={text.noTool}
            onChange={(flatDepth) => setState((current) => ({ ...current, flatDepth }))}
            onCheck={(useFlatDepth) => setState((current) => ({ ...current, useFlatDepth }))}
            unit={unit}
            value={state.flatDepth}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="sketch-toolpath__group">
        <legend className="sketch-toolpath__legend">{text.tool}</legend>
        <ToolPicker
          kinds={SKETCH_TOOL_KINDS}
          onChange={form.setTool}
          prefer="vBit"
          tool={form.tool}
          units={units}
        />
        {form.tool !== null && !carving ? (
          <p className="sketch-toolpath__notice">{sketch.notCarving}</p>
        ) : null}
      </fieldset>
      <SketchSourceFields
        errors={shown}
        handle={handle}
        onChange={setSource}
        part="limits"
        state={state.source}
        units={units}
      />
      <SafeZReadout />
      <ProjectField checked={form.project} onChange={form.setProject} />
      <VectorSelectionField
        filter={state.selector}
        onChange={(selector) => setState((current) => ({ ...current, selector }))}
      />
      <label className="sketch-toolpath__name" htmlFor={nameId}>
        {text.name}
        <input
          className="sketch-toolpath__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={sketch.title}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={carving ? undefined : sketch.notCarving}
        primaryLabel={text.calculate}
        submit
      />
    </form>
  );
}
