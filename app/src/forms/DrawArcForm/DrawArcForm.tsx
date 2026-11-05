import { useCallback, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { Point } from '../../bridge/document';
import { quickStepPoint } from '../../lib/quickKeys';
import { strings } from '../../lib/formStrings';
import { formatLength, toMillimetres, unitSuffix } from '../../lib/units';
import { useDrawController, usePreview } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  arcFromPoints,
  arcPathFromState,
  arcPreview,
  arcRadiusOf,
  clearPlaced,
  convertArcUnits,
  fillFromArc,
  initialArcState,
  placePoint,
  validateArc,
  withSizeBy,
  type ArcField,
  type ArcMode,
  type ArcSize,
} from './model';

import './DrawArcForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { arcModePicture } from '../HelperPicture/pictures/drawing';

const MODES: ReadonlyArray<{ value: ArcMode; label: string }> = [
  { value: 'threePoints', label: strings.createVectors.arc.threePoints },
  { value: 'centreStartEnd', label: strings.createVectors.arc.centreStartEnd },
];

const SIZES: ReadonlyArray<{ value: ArcSize; label: string }> = [
  { value: 'radius', label: strings.createVectors.arc.byRadius },
  { value: 'height', label: strings.createVectors.arc.byHeight },
];

export function DrawArcForm(): JSX.Element {
  const createPath = useDocumentStore((store) => store.createPath);
  const closeForm = useUiStore((store) => store.closeForm);
  const text = strings.createVectors.arc;
  const { state, setState, stateRef, units, unitsRef } = useFormState(
    initialArcState,
    convertArcUnits,
  );
  const [cursor, setCursor] = useState<Point | null>(null);
  const [showErrors, setShowErrors] = useState(false);

  const errors = useMemo(() => validateArc(state, units), [state, units]);
  const typedPath = useMemo(() => arcPathFromState(state, units), [state, units]);
  usePreview(
    useMemo(() => {
      if (state.placed.length > 0) {
        return arcPreview(state, cursor);
      }
      return typedPath === null ? [] : [typedPath];
    }, [state, cursor, typedPath]),
  );

  const create = useCallback(async () => {
    setShowErrors(true);
    const built = arcPathFromState(stateRef.current, unitsRef.current);
    if (built !== null) {
      await createPath(built);
    }
  }, [createPath, stateRef, unitsRef]);

  const controller = useMemo<DrawController>(() => {
    const place = (point: Point) => {
      const current = stateRef.current;
      if (current.placed.length < 2) {
        setState((s) => placePoint(s, point));
        return;
      }
      const [a, b] = current.placed;
      if (a === undefined || b === undefined) {
        return;
      }
      const built = arcFromPoints(current.mode, a, b, point);
      setState((s) => {
        const cleared = clearPlaced(s);
        return built === null ? cleared : fillFromArc(cleared, unitsRef.current, built);
      });
      if (built !== null) {
        void createPath(built);
      }
    };
    return {
      pointerDown: () => undefined,
      pointerMove: (p) => setCursor({ x: p.x, y: p.y }),
      pointerUp: (p) => place({ x: p.x, y: p.y }),
      // A right click finishes an arc whose first two points are down, at
      // the pointer, and then the tool closes as usual.
      rightClick: (at) => {
        if (stateRef.current.placed.length === 2) {
          place(at);
        }
        return false;
      },
      quickEntry: (entry, cursor) => {
        const from = stateRef.current.placed.at(-1);
        if (from === undefined) {
          return false;
        }
        const units = unitsRef.current;
        const point = quickStepPoint(entry, from, cursor, (v) => toMillimetres(v, units));
        if (point === null) {
          return false;
        }
        place(point);
        return true;
      },
      keyDown: (event) => {
        if (event.key === 'Enter') {
          void create();
          return true;
        }
        return false;
      },
      cancel: () => {
        setState((s) => clearPlaced(s));
      },
    };
  }, [create, createPath, setState, stateRef, unitsRef]);
  useDrawController(controller);

  const setField = (field: ArcField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: ArcField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);
  const radius = arcRadiusOf(typedPath);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void create();
  };

  const pointFields = (
    [
      ['startX', 'startY', text.pointStart],
      ['endX', 'endY', text.pointEnd],
    ] as const
  ).map(([xField, yField, label]) => (
    <fieldset className="draw-arc__group" key={label}>
      <legend className="draw-arc__legend">{label}</legend>
      <NumberField
        error={errorFor(xField)}
        label="X"
        onChange={(v) => setField(xField, v)}
        unit={unit}
        value={state[xField]}
      />
      <NumberField
        error={errorFor(yField)}
        label="Y"
        onChange={(v) => setField(yField, v)}
        unit={unit}
        value={state[yField]}
      />
    </fieldset>
  ));

  return (
    <form className="draw-arc" onSubmit={onSubmit}>
      <p className="draw-arc__hint">
        {state.mode === 'threePoints' ? text.hint3 : text.hintCentre}
      </p>
      <fieldset className="draw-arc__group">
        <legend className="draw-arc__legend">{text.mode}</legend>
        <HelperRow picture={<HelperPicture picture={arcModePicture(state.mode)} />}>
          <div className="draw-arc__choices">
            {MODES.map((mode) => (
              <label className="draw-arc__choice" key={mode.value}>
                <input
                  checked={state.mode === mode.value}
                  name="arcMode"
                  onChange={() => setState((s) => clearPlaced({ ...s, mode: mode.value }))}
                  type="radio"
                  value={mode.value}
                />
                {mode.label}
              </label>
            ))}
          </div>
        </HelperRow>
      </fieldset>
      <h3 className="draw-arc__legend">{text.exact}</h3>
      {pointFields}
      <fieldset className="draw-arc__group">
        <legend className="draw-arc__legend">{text.sizeBy}</legend>
        <div className="draw-arc__choices">
          {SIZES.map((size) => (
            <label className="draw-arc__choice" key={size.value}>
              <input
                checked={state.sizeBy === size.value}
                name="arcSize"
                onChange={() => setState((s) => withSizeBy(s, unitsRef.current, size.value))}
                type="radio"
                value={size.value}
              />
              {size.label}
            </label>
          ))}
        </div>
        <NumberField
          error={errorFor('size')}
          label={state.sizeBy === 'radius' ? text.byRadius : text.byHeight}
          onChange={(v) => setField('size', v)}
          unit={unit}
          value={state.size}
        />
        <p className="draw-arc__radius">{text.sizeHint}</p>
      </fieldset>
      <p className="draw-arc__radius">
        {text.radius}: {radius === null ? '-' : `${formatLength(radius, units)} ${unit}`}
      </p>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => void create()}
        primaryLabel={strings.form.create}
        submit
      />
    </form>
  );
}
