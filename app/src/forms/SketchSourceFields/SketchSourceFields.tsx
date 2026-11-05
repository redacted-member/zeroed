import type { JSX } from 'react';

import type { SketchBoundary, SketchSource } from '../../bridge/sketch';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useModelStore } from '../../state/modelStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { NumberField } from '../NumberField/NumberField';
import { SelectField } from '../SelectField/SelectField';
import { SliderBoxField } from '../SliderBoxField/SliderBoxField';
import { shownName } from '../TraceBitmapForm/model';
import {
  showsFilePlacement,
  usesFile,
  withBoundary,
  type SketchSourceErrors,
  type SketchSourceState,
} from './model';
import type { SketchSourceHandle } from './useSketchSource';

import './SketchSourceFields.scss';

const text = strings.toolpaths.sketch;
const laser = strings.toolpaths.laser;

const SOURCES: ReadonlyArray<Choice<SketchSource>> = [
  { value: 'bitmap', label: text.bitmap },
  { value: 'model', label: text.model },
];

/** The picture's own outline is named after what is being sketched. */
function boundaries(source: SketchSource): ReadonlyArray<Choice<SketchBoundary>> {
  return [
    { value: 'source', label: source === 'model' ? text.modelBoundary : text.bitmapBoundary },
    { value: 'vector', label: text.vectorBoundary },
    { value: 'level', label: text.levelBoundary },
  ];
}

interface SketchSourceFieldsProps {
  /** The Selection section, or the boundary and tracing sections below the tool. */
  part: 'selection' | 'limits';
  state: SketchSourceState;
  onChange: (update: (current: SketchSourceState) => SketchSourceState) => void;
  units: Units;
  errors: SketchSourceErrors;
  handle: SketchSourceHandle;
}

/** What a sketch toolpath traces, where it may go and how heavy its lines are. */
export function SketchSourceFields({
  part,
  state,
  onChange,
  units,
  errors,
  handle,
}: SketchSourceFieldsProps): JSX.Element {
  const levels = useModelStore((store) => store.levels);
  const activeLevel = useModelStore((store) => store.activeLevel);
  const unit = unitSuffix(units);
  const set =
    <K extends keyof SketchSourceState>(field: K) =>
    (value: SketchSourceState[K]) =>
      onChange((current) => ({ ...current, [field]: value }));

  if (part === 'selection') {
    const file = usesFile(state);
    return (
      <fieldset className="sketch-source">
        <legend className="sketch-source__legend">{text.selection}</legend>
        <ChoiceField
          label={text.selection}
          onChange={set('source')}
          options={SOURCES}
          value={state.source}
        />
        {state.source === 'bitmap' && handle.bitmapName !== null ? (
          <p className="sketch-source__hint">
            {handle.bitmapName === '' ? text.placedUnnamed : text.placed(handle.bitmapName)}
          </p>
        ) : null}
        {state.source === 'bitmap' ? (
          <>
            {file ? (
              <p className="sketch-source__hint">
                {state.path.trim() === '' ? text.pickBitmap : shownName(state.path)}
              </p>
            ) : null}
            {errors.path !== undefined && state.path !== '' ? (
              <p className="sketch-source__notice">{errors.path}</p>
            ) : null}
          </>
        ) : null}
        {showsFilePlacement(state) ? (
          <>
            <NumberField
              error={errors.left}
              label={laser.left}
              onChange={set('left')}
              unit={unit}
              value={state.left}
            />
            <NumberField
              error={errors.bottom}
              label={laser.bottom}
              onChange={set('bottom')}
              unit={unit}
              value={state.bottom}
            />
            <NumberField
              error={errors.width}
              label={laser.width}
              onChange={set('width')}
              unit={unit}
              value={state.width}
            />
            <NumberField
              error={errors.height}
              label={laser.height}
              onChange={set('height')}
              unit={unit}
              value={state.height}
            />
          </>
        ) : null}
      </fieldset>
    );
  }

  return (
    <>
      <fieldset className="sketch-source">
        <legend className="sketch-source__legend">{text.boundary}</legend>
        <ChoiceField
          label={text.boundary}
          onChange={(boundary) =>
            onChange((current) => withBoundary(current, boundary, activeLevel))
          }
          options={boundaries(state.source)}
          value={state.boundary}
        />
        <SelectField
          disabled={state.boundary !== 'level'}
          label={text.level}
          onChange={(id) => onChange((current) => ({ ...current, level: Number(id) }))}
          options={levels.map((level) => ({ value: String(level.id), label: level.name }))}
          value={state.level === null ? '' : String(state.level)}
        />
        <NumberField
          error={errors.boundaryOffset}
          label={text.boundaryOffset}
          onChange={set('boundaryOffset')}
          unit={unit}
          value={state.boundaryOffset}
        />
        {errors.boundary !== undefined ? (
          <p className="sketch-source__notice">{errors.boundary}</p>
        ) : null}
      </fieldset>
      <fieldset className="sketch-source">
        <legend className="sketch-source__legend">{text.tracing}</legend>
        <SliderBoxField
          endLabel={text.heavy}
          label={text.lineThickness}
          max={100}
          min={0}
          onChange={(value) => set('lineThickness')(Math.round(value))}
          startLabel={text.detail}
          step={1}
          value={state.lineThickness}
        />
        <p className="sketch-source__hint" role="status">
          {handle.tracing ? text.tracingNow : handle.areas !== null ? text.areas(handle.areas) : ''}
        </p>
        {handle.failure !== null ? <p className="sketch-source__notice">{handle.failure}</p> : null}
      </fieldset>
    </>
  );
}
