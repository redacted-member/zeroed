import { useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { lengthValue } from '../../lib/formFields';
import { circlePath } from '../../lib/geometry';
import { useDrawController, usePreview } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import type { DrawController, DrawPointer } from '../../state/drawStore';
import { useViewStore } from '../../state/viewStore';
import { selectionFrame } from '../../lib/transformMath';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertRotateUnits,
  initialRotateState,
  boxPivot,
  pivotAt,
  pivotFromSelection,
  snapPivot,
  rotateMatrixFromState,
  switchRotateMode,
  validateRotate,
  type RotateField,
  type RotateMode,
  type RotatePivot,
} from './model';

import './RotateForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { rotate } from '../HelperPicture/pictures/drawing';

const PIVOTS: ReadonlyArray<{ value: RotatePivot; label: string }> = [
  { value: 'topLeft', label: strings.transform.rotate.pivotTopLeft },
  { value: 'topRight', label: strings.transform.rotate.pivotTopRight },
  { value: 'centre', label: strings.transform.rotate.pivotCentre },
  { value: 'bottomLeft', label: strings.transform.rotate.pivotBottomLeft },
  { value: 'bottomRight', label: strings.transform.rotate.pivotBottomRight },
  { value: 'point', label: strings.transform.rotate.pivotPoint },
];

const MODES: ReadonlyArray<{ value: RotateMode; label: string }> = [
  { value: 'relative', label: strings.transform.rotate.relative },
  { value: 'absolute', label: strings.transform.rotate.absolute },
];

export function RotateForm(): JSX.Element {
  const selection = useDocumentStore((store) => store.selection);
  const transformSelection = useDocumentStore((store) => store.transformSelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const text = strings.transform.rotate;
  const { state, setState, units, unitsRef } = useFormState(initialRotateState, convertRotateUnits);
  const [showErrors, setShowErrors] = useState(false);
  const frame = useMemo(() => selectionFrame(selection), [selection]);

  useEffect(() => {
    setState((current) => pivotFromSelection(current, unitsRef.current, frame));
  }, [frame, setState, unitsRef]);

  // The pivot marker: two circles's pivot handle.
  const pivotPoint = useMemo(() => {
    if (frame === null) {
      return null;
    }
    if (state.pivot !== 'point') {
      return boxPivot(state.pivot, frame);
    }
    const x = lengthValue(state.pivotX, units);
    const y = lengthValue(state.pivotY, units);
    return x === null || y === null ? null : { x, y };
  }, [frame, state.pivot, state.pivotX, state.pivotY, units]);
  const marker = useMemo(() => {
    if (pivotPoint === null) {
      return [];
    }
    const px = useViewStore.getState().camera.pxPerMm;
    return [
      circlePath(pivotPoint.x, pivotPoint.y, 4 / px),
      circlePath(pivotPoint.x, pivotPoint.y, 8 / px),
    ];
  }, [pivotPoint]);
  usePreview(marker);
  const pivotRef = useRef(pivotPoint);
  useEffect(() => {
    pivotRef.current = pivotPoint;
  }, [pivotPoint]);
  const dragging = useRef(false);
  const controller = useMemo<DrawController>(() => {
    // The pivot follows the snapping options (Shift turns them off) and
    // also snaps to the selection box's points.
    const place = (p: DrawPointer) => {
      const tolerance = 8 / useViewStore.getState().camera.pxPerMm;
      const point = p.shift ? { x: p.rawX, y: p.rawY } : { x: p.x, y: p.y };
      const at = p.shift ? point : snapPivot(frame, point, tolerance);
      setState((current) => pivotAt(current, unitsRef.current, at));
    };
    return {
      pointerDown: (p) => {
        const pivot = pivotRef.current;
        const reach = 10 / useViewStore.getState().camera.pxPerMm;
        dragging.current =
          pivot !== null && Math.hypot(p.rawX - pivot.x, p.rawY - pivot.y) <= reach;
      },
      pointerMove: (p) => {
        if (dragging.current) {
          place(p);
        }
      },
      pointerUp: (p) => {
        // A drag of the handle, or a click anywhere, places the pivot.
        dragging.current = false;
        place(p);
      },
      cancel: () => {
        dragging.current = false;
      },
    };
  }, [frame, setState, unitsRef]);
  useDrawController(controller);

  const errors = useMemo(() => validateRotate(state, units), [state, units]);

  const apply = async () => {
    setShowErrors(true);
    if (frame === null) {
      return;
    }
    const step = rotateMatrixFromState(state, units, frame);
    if (step === null) {
      return;
    }
    await transformSelection(step);
  };

  const setField = (field: RotateField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: RotateField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void apply();
  };

  return (
    <form className="rotate-form" onSubmit={onSubmit}>
      {frame === null ? (
        <p className="rotate-form__notice">{strings.transform.noSelection}</p>
      ) : null}
      <fieldset className="rotate-form__group">
        <legend className="rotate-form__legend">{text.pivot}</legend>
        <HelperRow picture={<HelperPicture picture={rotate} />}>
          <p className="rotate-form__notice">{text.pivotHint}</p>
          <div className="rotate-form__choices">
            {PIVOTS.map((pivot) => (
              <label className="rotate-form__choice" key={pivot.value}>
                <input
                  checked={state.pivot === pivot.value}
                  name="rotatePivot"
                  onChange={() =>
                    setState((current) =>
                      pivotFromSelection(
                        { ...current, pivot: pivot.value },
                        unitsRef.current,
                        frame,
                      ),
                    )
                  }
                  type="radio"
                  value={pivot.value}
                />
                {pivot.label}
              </label>
            ))}
          </div>
          <NumberField
            disabled={state.pivot !== 'point'}
            error={errorFor('pivotX')}
            label={text.pivotX}
            onChange={(v) => setField('pivotX', v)}
            unit={unit}
            value={state.pivotX}
          />
          <NumberField
            disabled={state.pivot !== 'point'}
            error={errorFor('pivotY')}
            label={text.pivotY}
            onChange={(v) => setField('pivotY', v)}
            unit={unit}
            value={state.pivotY}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="rotate-form__group">
        <legend className="rotate-form__legend">{text.typeAndAngle}</legend>
        <div className="rotate-form__choices">
          {MODES.map((mode) => (
            <label className="rotate-form__choice" key={mode.value}>
              <input
                checked={state.mode === mode.value}
                name="rotateMode"
                onChange={() => setState((current) => switchRotateMode(current, mode.value, frame))}
                type="radio"
                value={mode.value}
              />
              {mode.label}
            </label>
          ))}
        </div>
        <NumberField
          error={errorFor('angle')}
          label={text.angle}
          onChange={(v) => setField('angle', v)}
          step={1}
          unit="deg"
          value={state.angle}
        />
      </fieldset>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => void apply()}
        primaryWhy={frame === null ? strings.transform.noSelection : undefined}
        primaryLabel={strings.form.apply}
        submit
      />
    </form>
  );
}
