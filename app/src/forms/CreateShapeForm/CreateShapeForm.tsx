import { useCallback, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { Combine } from '../../bridge/model';
import { api } from '../../bridge/formApi';
import { formatLength, unitSuffix } from '../../lib/units';
import { strings } from '../../lib/formStrings';
import { numberValue } from '../../lib/formFields';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { type Choice } from '../ChoiceField/ChoiceField';
import { DynamicFields } from '../DynamicFields/DynamicFields';
import type { DynamicState } from '../DynamicFields/model';
import { ModelingActions } from '../ModelingActions/ModelingActions';
import { MODELING_COMBINES } from '../ModelingActions/model';
import { useModelingApply } from '../ModelingActions/useModelingApply';
import { NumberField } from '../NumberField/NumberField';
import { SliderField } from '../SliderField/SliderField';
import {
  canApply,
  convertShapeUnits,
  editableShape,
  fieldsOf,
  initialShapeState,
  shapeArgs,
  shapeVectors,
  stateFromComponent,
  toolText,
  validateShape,
  withTotalHeight,
  type ShapeField,
  type ShapeFormState,
  type ShapeTool,
} from './model';

import './CreateShapeForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { shapeHeights, shapeProfilePicture } from '../HelperPicture/pictures/modeling';

const text = strings.model.createShape;

/** Combine choices of the other creation forms. */
export const COMBINES: ReadonlyArray<Choice<Combine>> = MODELING_COMBINES;

function ShapeForm({ tool }: { tool: ShapeTool }): JSX.Element {
  const selection = useDocumentStore((store) => store.selection.ids);
  const entities = useDocumentStore((store) => store.entities);
  const closeForm = useUiStore((store) => store.closeForm);
  const [editing] = useState(() => {
    const model = useModelStore.getState();
    return editableShape(model.components, pickedComponents(model), tool);
  });
  const initial = useCallback(
    (units: Parameters<typeof initialShapeState>[0]) =>
      editing === null ? initialShapeState(units, tool) : stateFromComponent(editing, units),
    [editing, tool],
  );
  const { state, setState, units } = useFormState<ShapeFormState>(initial, convertShapeUnits);
  const { track, apply, reset, startNew } = useModelingApply(editing?.id ?? null);
  const [showErrors, setShowErrors] = useState(false);
  const [totalText, setTotalText] = useState<string | null>(null);
  const errors = useMemo(() => validateShape(state, tool, units), [state, tool, units]);
  const fields = fieldsOf(tool);
  const unit = unitSuffix(units);
  const ids = shapeVectors(selection, editing);
  const { hint } = toolText(tool);

  const set = (field: ShapeField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: ShapeField) => (showErrors ? errors[field] : undefined);
  const onDynamic = useCallback(
    (dynamic: DynamicState) => setState((current) => ({ ...current, dynamic })),
    [setState],
  );

  const run = (next: ShapeFormState) => {
    setShowErrors(true);
    const args = shapeArgs(next, tool, units, ids);
    if (args === null || !canApply(tool, ids, entities)) {
      return;
    }
    void apply((target) => api.model.createShape({ ...args, ...target })).then(() => {
      setTotalText(null);
      if (useUiStore.getState().viewMode === '2d') {
        useUiStore.getState().setViewMode('split');
      }
    });
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    run(state);
  };

  const applyTotal = () => {
    if (totalText === null) {
      return;
    }
    const next = withTotalHeight(state, totalText, track.shapeHeight, units);
    setState(next);
    run(next);
  };

  const totalShown =
    totalText ?? (track.totalHeight === null ? '' : formatLength(track.totalHeight, units));

  return (
    <form className="create-shape" onSubmit={onSubmit}>
      <p className="create-shape__hint">{editing === null ? hint : text.editing(editing.name)}</p>
      {canApply(tool, ids, entities) ? null : (
        <p className="create-shape__notice">
          {tool === 'custom' ? text.needsProfile : text.needsSelection}
        </p>
      )}
      {tool === 'flat' && ids.length === 0 ? (
        <p className="create-shape__hint">{text.wholeMaterial}</p>
      ) : null}
      {fields.angle || fields.smoothing || fields.limit ? (
        <fieldset className="create-shape__group">
          <legend className="create-shape__legend">{text.profile}</legend>
          <HelperRow picture={<HelperPicture picture={shapeProfilePicture(tool)} />}>
            {fields.angle ? (
              <>
                <NumberField
                  error={errorFor('angle')}
                  label={text.angle}
                  onChange={(v) => set('angle', v)}
                  unit={strings.units.degrees}
                  value={state.angle}
                />
                <SliderField
                  label={text.angleSlider}
                  max={90}
                  min={1}
                  onChange={(v) => set('angle', String(v))}
                  unit={strings.units.degrees}
                  value={Math.min(90, Math.max(1, numberValue(state.angle) ?? 90))}
                />
              </>
            ) : null}
            {fields.smoothing ? (
              <>
                <SliderField
                  label={text.smoothTop}
                  max={100}
                  min={0}
                  onChange={(v) => set('smoothTop', String(v))}
                  unit="%"
                  value={numberValue(state.smoothTop) ?? 50}
                />
                <SliderField
                  label={text.smoothBottom}
                  max={100}
                  min={0}
                  onChange={(v) => set('smoothBottom', String(v))}
                  unit="%"
                  value={numberValue(state.smoothBottom) ?? 50}
                />
              </>
            ) : null}
            {fields.limit ? (
              <>
                <CheckField
                  checked={state.limitOn}
                  label={text.limit}
                  onChange={(limitOn) => setState((current) => ({ ...current, limitOn }))}
                />
                <NumberField
                  disabled={!state.limitOn}
                  error={errorFor('limit')}
                  label={text.limitHeight}
                  onChange={(v) => set('limit', v)}
                  unit={unit}
                  value={state.limit}
                />
              </>
            ) : null}
            {fields.sharpCorners ? (
              <CheckField
                checked={state.sharpCorners}
                label={text.sharpCorners}
                onChange={(sharpCorners) => setState((current) => ({ ...current, sharpCorners }))}
              />
            ) : null}
            {fields.blendInner ? (
              <CheckField
                checked={state.blendInner}
                label={text.blendInner}
                onChange={(blendInner) => setState((current) => ({ ...current, blendInner }))}
              />
            ) : null}
          </HelperRow>
        </fieldset>
      ) : null}
      <fieldset className="create-shape__group">
        <legend className="create-shape__legend">{text.heights}</legend>
        <HelperRow
          picture={
            <HelperPicture picture={tool === 'flat' ? shapeProfilePicture('flat') : shapeHeights} />
          }
        >
          {fields.scale ? (
            <>
              <CheckField
                checked={state.scaleOn}
                label={text.scale}
                onChange={(scaleOn) => setState((current) => ({ ...current, scaleOn }))}
              />
              <NumberField
                disabled={!state.scaleOn}
                error={errorFor('scaleHeight')}
                label={text.scaleHeight}
                onChange={(v) => set('scaleHeight', v)}
                unit={unit}
                value={state.scaleHeight}
              />
            </>
          ) : null}
          <NumberField
            error={errorFor('baseHeight')}
            label={text.baseHeight}
            onChange={(v) => set('baseHeight', v)}
            unit={unit}
            value={state.baseHeight}
          />
        </HelperRow>
      </fieldset>
      <DynamicFields onChange={onDynamic} showErrors={showErrors} state={state.dynamic} />
      {fields.totalHeight ? (
        <NumberField
          label={text.totalHeight}
          onChange={setTotalText}
          onEnter={applyTotal}
          placeholder={text.notApplied}
          unit={unit}
          value={totalShown}
        />
      ) : null}
      <ModelingActions
        applyWhy={
          canApply(tool, ids, entities)
            ? undefined
            : tool === 'custom'
              ? text.needsProfile
              : text.needsSelection
        }
        combine={state.combine}
        name={state.name}
        onClose={closeForm}
        onCombine={(combine) => setState((current) => ({ ...current, combine }))}
        onName={(name) => setState((current) => ({ ...current, name }))}
        onReset={() => void reset()}
        onStartNew={() => void startNew()}
      />
    </form>
  );
}

export function CreateFlatShapeForm(): JSX.Element {
  return <ShapeForm tool="flat" />;
}

export function CreateShapeForm(): JSX.Element {
  return <ShapeForm tool="round" />;
}

export function CreateAngledShapeForm(): JSX.Element {
  return <ShapeForm tool="angled" />;
}

export function CreateSmoothShapeForm(): JSX.Element {
  return <ShapeForm tool="smooth" />;
}

export function CreateCustomShapeForm(): JSX.Element {
  return <ShapeForm tool="custom" />;
}
