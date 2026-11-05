import { useCallback, useMemo, useState, type FormEvent, type JSX } from 'react';

import { recognisePolygon, useShapeEditor, type PolygonShape } from '../../lib/shapeEdit';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDrawController, usePreview } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useShapeDrag } from '../../lib/useShapeDrag';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertPolygonUnits,
  initialPolygonState,
  polygonAt,
  polygonFromDrag,
  polygonPathFromState,
  validatePolygon,
  type PolygonField,
  polygonFromQuick,
  polygonFromShape,
  polygonCentredOn,
  type PolygonFormState,
  switchPolygonSizeBy,
  type PolygonSizeBy,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './DrawPolygonForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { polygonSizePicture } from '../HelperPicture/pictures/drawing';

const SIZE_BY: ReadonlyArray<Choice<PolygonSizeBy>> = [
  { value: 'radius', label: strings.createVectors.polygon.radius },
  { value: 'diameter', label: strings.createVectors.polygon.diameter },
];

export function DrawPolygonForm(): JSX.Element {
  const createPath = useDocumentStore((store) => store.createPath);
  const closeForm = useUiStore((store) => store.closeForm);
  const text = strings.createVectors.polygon;
  const { state, setState, stateRef, units, unitsRef } = useFormState(
    initialPolygonState,
    convertPolygonUnits,
  );
  const [showErrors, setShowErrors] = useState(false);
  const editor = useShapeEditor<PolygonFormState, PolygonShape>({
    recognise: recognisePolygon,
    fill: polygonFromShape,
    place: polygonCentredOn,
    build: polygonPathFromState,
    setState,
    stateRef,
    units,
    unitsRef,
  });

  const errors = useMemo(() => validatePolygon(state, units), [state, units]);
  const path = useMemo(() => polygonPathFromState(state, units), [state, units]);
  usePreview(useMemo(() => (path === null ? [] : [path]), [path]));

  const create = useCallback(async () => {
    setShowErrors(true);
    const built = polygonPathFromState(stateRef.current, unitsRef.current);
    if (built !== null) {
      await createPath(built);
    }
  }, [createPath, stateRef, unitsRef]);

  const controller = useShapeDrag(
    useMemo(
      () => ({
        onDrag: (from, to, p) =>
          setState((current) =>
            polygonFromDrag(current, unitsRef.current, from, to, p.alt === true),
          ),
        onCommit: (from, to, p) => {
          const next = polygonFromDrag(
            stateRef.current,
            unitsRef.current,
            from,
            to,
            p.alt === true,
          );
          setState(next);
          const built = polygonPathFromState(next, unitsRef.current);
          if (built !== null) {
            void createPath(built);
          }
        },
        onClick: (point, pointer) =>
          editor.click(point, pointer, () =>
            setState((current) => polygonAt(current, unitsRef.current, point)),
          ),
        onEnter: () => void create(),
        onQuick: (from, _cursor, entry) => {
          const next = polygonFromQuick(stateRef.current, unitsRef.current, from, entry);
          if (next === null) {
            return false;
          }
          setState(next);
          const built = polygonPathFromState(next, unitsRef.current);
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

  const setField = (field: PolygonField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: PolygonField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void create();
  };

  return (
    <form className="draw-polygon" onSubmit={onSubmit}>
      <p className="draw-polygon__hint">{text.hint}</p>
      <fieldset className="draw-polygon__group">
        <legend className="draw-polygon__legend">{text.title}</legend>
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
        <NumberField
          error={errorFor('sides')}
          label={text.sides}
          onChange={(v) => setField('sides', v)}
          step={1}
          value={state.sides}
        />
        <HelperRow picture={<HelperPicture picture={polygonSizePicture(state.sizeBy)} />}>
          <ChoiceField
            label={text.sizeBy}
            onChange={(sizeBy) =>
              setState((current) => switchPolygonSizeBy(current, unitsRef.current, sizeBy))
            }
            options={SIZE_BY}
            value={state.sizeBy}
          />
          <NumberField
            error={errorFor('radius')}
            label={state.sizeBy === 'radius' ? text.radius : text.diameter}
            onChange={(v) => setField('radius', v)}
            unit={unit}
            value={state.radius}
          />
        </HelperRow>
      </fieldset>
      {editor.count > 0 ? (
        <>
          <p className="draw-polygon__hint" role="status">
            {strings.createVectors.editing(editor.count)}
          </p>
          <button
            className="draw-polygon__button"
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
