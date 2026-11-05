import { useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { lengthText, lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { circlePath } from '../../lib/geometry';
import { useDrawController, usePreview } from '../../lib/useDrawController';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController, DrawPointer } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { SelectionSizeField } from '../SelectionSizeField/SelectionSizeField';
import {
  centreFromSelection,
  circularArrayArgs,
  convertCircularArrayUnits,
  initialCircularArrayState,
  validateCircularArray,
  type AngleMode,
  type CircularArrayField,
} from './model';
import { snapPivot } from '../RotateForm/model';

import './CircularArrayForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { circularArray } from '../HelperPicture/pictures/drawing';

const text = strings.editObjects.circularArray;

const ANGLE_MODES: ReadonlyArray<Choice<AngleMode>> = [
  { value: 'total', label: text.angle },
  { value: 'step', label: text.step },
];

export function CircularArrayForm(): JSX.Element {
  const selection = useDocumentStore((store) => store.selection);
  const circularArraySelection = useDocumentStore((store) => store.circularArraySelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(
    initialCircularArrayState,
    convertCircularArrayUnits,
  );
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateCircularArray(state, units), [state, units]);
  const unit = unitSuffix(units);
  // Vectors, components or both: the box covers whatever is selected.
  const hasSelection = selection.bbox !== null;
  const bbox = selection.bbox;

  useEffect(() => {
    setState((current) => centreFromSelection(current, unitsRef.current, bbox));
  }, [bbox, setState, unitsRef]);

  // The rotation center marker, dragged (or placed by a click) in the view.
  const centre = useMemo(() => {
    const x = lengthValue(state.cx, units);
    const y = lengthValue(state.cy, units);
    return x === null || y === null ? null : { x, y };
  }, [state.cx, state.cy, units]);
  const marker = useMemo(() => {
    if (centre === null || bbox === null) {
      return [];
    }
    const px = useViewStore.getState().camera.pxPerMm;
    return [circlePath(centre.x, centre.y, 4 / px), circlePath(centre.x, centre.y, 8 / px)];
  }, [bbox, centre]);
  usePreview(marker);
  const centreRef = useRef(centre);
  useEffect(() => {
    centreRef.current = centre;
  }, [centre]);
  const dragging = useRef(false);
  const controller = useMemo<DrawController>(() => {
    // Snaps to the selection box's corners, edge middles and middle; Shift
    // places it freely.
    const place = (p: DrawPointer) => {
      const tolerance = 8 / useViewStore.getState().camera.pxPerMm;
      const frame = bbox === null ? null : { angle: 0, extent: bbox };
      const at = p.shift
        ? { x: p.rawX, y: p.rawY }
        : snapPivot(frame, { x: p.x, y: p.y }, tolerance);
      setState((current) => ({
        ...current,
        cx: lengthText(at.x, unitsRef.current),
        cy: lengthText(at.y, unitsRef.current),
      }));
    };
    return {
      pointerDown: (p) => {
        const at = centreRef.current;
        const reach = 10 / useViewStore.getState().camera.pxPerMm;
        dragging.current = at !== null && Math.hypot(p.rawX - at.x, p.rawY - at.y) <= reach;
      },
      pointerMove: (p) => {
        if (dragging.current) {
          place(p);
        }
      },
      pointerUp: (p) => {
        dragging.current = false;
        place(p);
      },
      cancel: () => {
        dragging.current = false;
      },
    };
  }, [bbox, setState, unitsRef]);
  useDrawController(controller);

  const setField = (field: CircularArrayField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: CircularArrayField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const args = circularArrayArgs(state, units);
    if (args !== null && hasSelection) {
      void circularArraySelection(args);
    }
  };

  return (
    <form className="circular-array-form" onSubmit={onSubmit}>
      <p className="circular-array-form__hint">{text.hint}</p>
      {hasSelection ? null : (
        <p className="circular-array-form__notice">{strings.editObjects.needsObjects}</p>
      )}
      <SelectionSizeField units={units} />
      <fieldset className="circular-array-form__group">
        <legend className="circular-array-form__legend">{text.centre}</legend>
        <HelperRow picture={<HelperPicture picture={circularArray} />}>
          <NumberField
            error={errorFor('cx')}
            label={text.x}
            onChange={(v) => setField('cx', v)}
            unit={unit}
            value={state.cx}
          />
          <NumberField
            error={errorFor('cy')}
            label={text.y}
            onChange={(v) => setField('cy', v)}
            unit={unit}
            value={state.cy}
          />
        </HelperRow>
      </fieldset>
      <CheckField
        checked={state.rotateCopies}
        label={text.rotate}
        onChange={(rotateCopies) => setState((current) => ({ ...current, rotateCopies }))}
      />
      <fieldset className="circular-array-form__group">
        <legend className="circular-array-form__legend">{text.angleGroup}</legend>
        <NumberField
          error={errorFor('count')}
          label={text.count}
          onChange={(v) => setField('count', v)}
          step={1}
          value={state.count}
        />
        <ChoiceField
          label={text.angleGroup}
          onChange={(angleMode) => setState((current) => ({ ...current, angleMode }))}
          options={ANGLE_MODES}
          value={state.angleMode}
        />
        <NumberField
          error={errorFor('angle')}
          label={state.angleMode === 'total' ? text.angle : text.step}
          onChange={(v) => setField('angle', v)}
          unit={strings.units.degrees}
          value={state.angle}
        />
      </fieldset>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={hasSelection ? undefined : strings.editObjects.needsObjects}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
