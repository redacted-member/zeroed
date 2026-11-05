import { useCallback, useMemo, useState, type FormEvent, type JSX } from 'react';

import { recogniseStar, useShapeEditor, type StarShape } from '../../lib/shapeEdit';
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
  convertStarUnits,
  initialStarState,
  starAt,
  starFromDrag,
  starPathFromState,
  validateStar,
  type StarField,
  starFromQuick,
  starFromShape,
  starCentredOn,
  type StarFormState,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './DrawStarForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { star } from '../HelperPicture/pictures/drawing';

export function DrawStarForm(): JSX.Element {
  const createPath = useDocumentStore((store) => store.createPath);
  const closeForm = useUiStore((store) => store.closeForm);
  const text = strings.createVectors.star;
  const { state, setState, stateRef, units, unitsRef } = useFormState(
    initialStarState,
    convertStarUnits,
  );
  const [showErrors, setShowErrors] = useState(false);
  const editor = useShapeEditor<StarFormState, StarShape>({
    recognise: recogniseStar,
    fill: starFromShape,
    place: starCentredOn,
    build: starPathFromState,
    setState,
    stateRef,
    units,
    unitsRef,
  });

  const errors = useMemo(() => validateStar(state, units), [state, units]);
  const path = useMemo(() => starPathFromState(state, units), [state, units]);
  usePreview(useMemo(() => (path === null ? [] : [path]), [path]));

  const create = useCallback(async () => {
    setShowErrors(true);
    const built = starPathFromState(stateRef.current, unitsRef.current);
    if (built !== null) {
      await createPath(built);
    }
  }, [createPath, stateRef, unitsRef]);

  const controller = useShapeDrag(
    useMemo(
      () => ({
        onDrag: (from, to, p) =>
          setState((current) => starFromDrag(current, unitsRef.current, from, to, p.alt === true)),
        onCommit: (from, to, p) => {
          const next = starFromDrag(stateRef.current, unitsRef.current, from, to, p.alt === true);
          setState(next);
          const built = starPathFromState(next, unitsRef.current);
          if (built !== null) {
            void createPath(built);
          }
        },
        onClick: (point, pointer) =>
          editor.click(point, pointer, () =>
            setState((current) => starAt(current, unitsRef.current, point)),
          ),
        onEnter: () => void create(),
        onQuick: (from, _cursor, entry) => {
          const next = starFromQuick(stateRef.current, unitsRef.current, from, entry);
          if (next === null) {
            return false;
          }
          setState(next);
          const built = starPathFromState(next, unitsRef.current);
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

  const setField = (field: StarField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: StarField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void create();
  };

  return (
    <form className="draw-star" onSubmit={onSubmit}>
      <p className="draw-star__hint">{text.hint}</p>
      <fieldset className="draw-star__group">
        <legend className="draw-star__legend">{text.title}</legend>
        <NumberField
          error={errorFor('points')}
          label={text.points}
          onChange={(v) => setField('points', v)}
          step={1}
          value={state.points}
        />
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
        <HelperRow picture={<HelperPicture picture={star} />}>
          <NumberField
            error={errorFor('outerRadius')}
            label={text.outerRadius}
            onChange={(v) => setField('outerRadius', v)}
            unit={unit}
            value={state.outerRadius}
          />
          <NumberField
            error={errorFor('inner')}
            label={text.innerPercent}
            onChange={(v) => setField('inner', v)}
            unit="%"
            value={state.inner}
          />
        </HelperRow>
      </fieldset>
      {editor.count > 0 ? (
        <>
          <p className="draw-star__hint" role="status">
            {strings.createVectors.editing(editor.count)}
          </p>
          <button
            className="draw-star__button"
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
