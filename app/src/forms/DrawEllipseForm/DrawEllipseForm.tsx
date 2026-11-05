import { useCallback, useMemo, useState, type FormEvent, type JSX } from 'react';

import { recogniseEllipse, useShapeEditor, type EllipseShape } from '../../lib/shapeEdit';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDrawController, usePreview } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useShapeDrag } from '../../lib/useShapeDrag';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { AnchorPicker } from '../AnchorPicker/AnchorPicker';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertEllipseUnits,
  ellipseAt,
  ellipseFromDrag,
  ellipsePathFromState,
  initialEllipseState,
  validateEllipse,
  type EllipseField,
  ellipseFromQuick,
  ellipseFromShape,
  ellipsePlacedOn,
  type EllipseFormState,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './DrawEllipseForm.scss';

export function DrawEllipseForm(): JSX.Element {
  const createPath = useDocumentStore((store) => store.createPath);
  const closeForm = useUiStore((store) => store.closeForm);
  const text = strings.createVectors.ellipse;
  const { state, setState, stateRef, units, unitsRef } = useFormState(
    initialEllipseState,
    convertEllipseUnits,
  );
  const [showErrors, setShowErrors] = useState(false);
  const editor = useShapeEditor<EllipseFormState, EllipseShape>({
    recognise: recogniseEllipse,
    fill: ellipseFromShape,
    place: ellipsePlacedOn,
    build: ellipsePathFromState,
    setState,
    stateRef,
    units,
    unitsRef,
  });

  const errors = useMemo(() => validateEllipse(state, units), [state, units]);
  const path = useMemo(() => ellipsePathFromState(state, units), [state, units]);
  usePreview(useMemo(() => (path === null ? [] : [path]), [path]));

  const create = useCallback(async () => {
    setShowErrors(true);
    const built = ellipsePathFromState(stateRef.current, unitsRef.current);
    if (built !== null) {
      await createPath(built);
    }
  }, [createPath, stateRef, unitsRef]);

  const controller = useShapeDrag(
    useMemo(
      () => ({
        onDrag: (from, to, p) =>
          setState((current) =>
            ellipseFromDrag(current, unitsRef.current, from, to, p.ctrl, p.alt === true),
          ),
        onCommit: (from, to, p) => {
          const next = ellipseFromDrag(
            stateRef.current,
            unitsRef.current,
            from,
            to,
            p.ctrl,
            p.alt === true,
          );
          setState(next);
          const built = ellipsePathFromState(next, unitsRef.current);
          if (built !== null) {
            void createPath(built);
          }
        },
        onClick: (point, pointer) =>
          editor.click(point, pointer, () =>
            setState((current) => ellipseAt(current, unitsRef.current, point)),
          ),
        onEnter: () => void create(),
        onQuick: (from, cursor, entry) => {
          const next = ellipseFromQuick(stateRef.current, unitsRef.current, from, cursor, entry);
          if (next === null) {
            return false;
          }
          setState(next);
          const built = ellipsePathFromState(next, unitsRef.current);
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

  const setField = (field: EllipseField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: EllipseField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void create();
  };

  return (
    <form className="draw-ellipse" onSubmit={onSubmit}>
      <p className="draw-ellipse__hint">{text.hint}</p>
      <fieldset className="draw-ellipse__group">
        <legend className="draw-ellipse__legend">{strings.form.anchor}</legend>
        <AnchorPicker
          label={strings.form.anchor}
          onChange={(anchor) => setState((current) => ({ ...current, anchor }))}
          value={state.anchor}
        />
        <NumberField
          error={errorFor('x')}
          label="X"
          onChange={(v) => setField('x', v)}
          unit={unit}
          value={state.x}
        />
        <NumberField
          error={errorFor('y')}
          label="Y"
          onChange={(v) => setField('y', v)}
          unit={unit}
          value={state.y}
        />
      </fieldset>
      <fieldset className="draw-ellipse__group">
        <legend className="draw-ellipse__legend">{text.size}</legend>
        <NumberField
          error={errorFor('width')}
          label={text.width}
          onChange={(v) => setField('width', v)}
          unit={unit}
          value={state.width}
        />
        <NumberField
          error={errorFor('height')}
          label={text.height}
          onChange={(v) => setField('height', v)}
          unit={unit}
          value={state.height}
        />
      </fieldset>
      {editor.count > 0 ? (
        <>
          <p className="draw-ellipse__hint" role="status">
            {strings.createVectors.editing(editor.count)}
          </p>
          <button
            className="draw-ellipse__button"
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
