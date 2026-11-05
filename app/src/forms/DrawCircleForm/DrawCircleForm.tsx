import { useCallback, useMemo, useState, type FormEvent, type JSX } from 'react';

import { recogniseCircle, useShapeEditor, type CircleShape } from '../../lib/shapeEdit';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDrawController, usePreview } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useShapeDrag } from '../../lib/useShapeDrag';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  circleAt,
  circleFromDrag,
  circleCentredOn,
  circleFromShape,
  circlePathFromState,
  convertCircleUnits,
  initialCircleState,
  switchSizeBy,
  validateCircle,
  type CircleField,
  type CircleFormState,
  type CircleSizeBy,
  circleFromQuick,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './DrawCircleForm.scss';

const SIZE_BY: ReadonlyArray<{ value: CircleSizeBy; label: string }> = [
  { value: 'radius', label: strings.createVectors.circle.radius },
  { value: 'diameter', label: strings.createVectors.circle.diameter },
];

export function DrawCircleForm(): JSX.Element {
  const createPath = useDocumentStore((store) => store.createPath);
  const closeForm = useUiStore((store) => store.closeForm);
  const text = strings.createVectors.circle;
  const { state, setState, stateRef, units, unitsRef } = useFormState(
    initialCircleState,
    convertCircleUnits,
  );
  const [showErrors, setShowErrors] = useState(false);
  const editor = useShapeEditor<CircleFormState, CircleShape>({
    recognise: recogniseCircle,
    fill: circleFromShape,
    place: circleCentredOn,
    build: circlePathFromState,
    setState,
    stateRef,
    units,
    unitsRef,
  });

  const errors = useMemo(() => validateCircle(state, units), [state, units]);
  const path = useMemo(() => circlePathFromState(state, units), [state, units]);
  usePreview(useMemo(() => (path === null ? [] : [path]), [path]));

  const create = useCallback(async () => {
    setShowErrors(true);
    const built = circlePathFromState(stateRef.current, unitsRef.current);
    if (built !== null) {
      await createPath(built);
    }
  }, [createPath, stateRef, unitsRef]);

  const controller = useShapeDrag(
    useMemo(
      () => ({
        onDrag: (from, to, p) =>
          setState((current) =>
            circleFromDrag(current, unitsRef.current, from, to, p.alt === true),
          ),
        onCommit: (from, to, p) => {
          const next = circleFromDrag(stateRef.current, unitsRef.current, from, to, p.alt === true);
          setState(next);
          const built = circlePathFromState(next, unitsRef.current);
          if (built !== null) {
            void createPath(built);
          }
        },
        onClick: (point, pointer) =>
          editor.click(point, pointer, () =>
            setState((current) => circleAt(current, unitsRef.current, point)),
          ),
        onEnter: () => void create(),
        onQuick: (from, _cursor, entry) => {
          const next = circleFromQuick(stateRef.current, unitsRef.current, from, entry);
          if (next === null) {
            return false;
          }
          setState(next);
          const built = circlePathFromState(next, unitsRef.current);
          if (built !== null) {
            void createPath(built);
          }
          return true;
        },
      }),
      [create, createPath, editor, setState, stateRef, unitsRef],
    ),
  );
  useDrawController(controller);

  const setField = (field: CircleField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: CircleField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void create();
  };

  return (
    <form className="draw-circle" onSubmit={onSubmit}>
      <p className="draw-circle__hint">{text.hint}</p>
      <fieldset className="draw-circle__group">
        <legend className="draw-circle__legend">{text.title}</legend>
        <NumberField
          error={errorFor('centreX')}
          label={text.centreX}
          onChange={(v) => setField('centreX', v)}
          unit={unit}
          value={state.centreX}
        />
        <NumberField
          error={errorFor('centreY')}
          label={text.centreY}
          onChange={(v) => setField('centreY', v)}
          unit={unit}
          value={state.centreY}
        />
        <div className="draw-circle__choices">
          {SIZE_BY.map((option) => (
            <label className="draw-circle__choice" key={option.value}>
              <input
                checked={state.sizeBy === option.value}
                name="sizeBy"
                onChange={() =>
                  setState((current) => switchSizeBy(current, unitsRef.current, option.value))
                }
                type="radio"
                value={option.value}
              />
              {option.label}
            </label>
          ))}
        </div>
        <NumberField
          error={errorFor('size')}
          label={state.sizeBy === 'radius' ? text.radius : text.diameter}
          onChange={(v) => setField('size', v)}
          unit={unit}
          value={state.size}
        />
      </fieldset>
      {editor.count > 0 ? (
        <>
          <p className="draw-circle__hint" role="status">
            {strings.createVectors.editing(editor.count)}
          </p>
          <button
            className="draw-circle__button"
            onClick={() => void editor.apply()}
            type="button"
            data-tip={strings.form.apply}
          >
            <ButtonFace icon="formApply" label={strings.form.apply} />
          </button>
        </>
      ) : null}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => void create()}
        primaryLabel={strings.form.create}
        submit
      />
    </form>
  );
}
