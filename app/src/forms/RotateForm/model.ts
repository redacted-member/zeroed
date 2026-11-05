// Pure state for Rotate: angle and pivot. A
// relative angle turns the selection by that much; an absolute angle sets
// the selection's kept rotation to it, so a rotated object can be put back
// square with 0.

import type { Matrix, Point, SelectionFrame } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import { bboxCentre, fromFrame, rotateMatrix } from '../../lib/transformMath';

/** The pivot: a corner or the centre of the selection box, or typed coordinates. */
export type RotatePivot =
  'topLeft' | 'topRight' | 'centre' | 'bottomLeft' | 'bottomRight' | 'point';
export type RotateMode = 'relative' | 'absolute';

export interface RotateFormState {
  mode: RotateMode;
  angle: string;
  pivot: RotatePivot;
  pivotX: string;
  pivotY: string;
}

export type RotateField = 'angle' | 'pivotX' | 'pivotY';
export type RotateErrors = Partial<Record<RotateField, string>>;

export function initialRotateState(units: Units): RotateFormState {
  return {
    mode: 'relative',
    angle: '45',
    pivot: 'centre',
    pivotX: lengthText(0, units),
    pivotY: lengthText(0, units),
  };
}

export function convertRotateUnits(
  state: RotateFormState,
  from: Units,
  to: Units,
): RotateFormState {
  return convertLengthFields(state, ['pivotX', 'pivotY'], from, to);
}

/** The centre of the selection's (possibly rotated) box, in job space. */
export function frameCentre(frame: SelectionFrame): Point {
  return fromFrame(frame, bboxCentre(frame.extent));
}

/** Switches between relative and absolute angles, showing the current rotation. */
export function switchRotateMode(
  state: RotateFormState,
  mode: RotateMode,
  frame: SelectionFrame | null,
): RotateFormState {
  if (mode === state.mode) {
    return state;
  }
  const current = Math.round((frame?.angle ?? 0) * 1000) / 1000;
  return { ...state, mode, angle: mode === 'absolute' ? String(current) : '45' };
}

/** The job point a box pivot stands for (a corner or the centre of the rotated box). */
export function boxPivot(pivot: Exclude<RotatePivot, 'point'>, frame: SelectionFrame): Point {
  const [x0, y0, x1, y1] = frame.extent;
  switch (pivot) {
    case 'topLeft':
      return fromFrame(frame, { x: x0, y: y1 });
    case 'topRight':
      return fromFrame(frame, { x: x1, y: y1 });
    case 'bottomLeft':
      return fromFrame(frame, { x: x0, y: y0 });
    case 'bottomRight':
      return fromFrame(frame, { x: x1, y: y0 });
    case 'centre':
      return frameCentre(frame);
  }
}

/** Puts the pivot fields at the chosen corner or centre of the selection. */
export function pivotFromSelection(
  state: RotateFormState,
  units: Units,
  frame: SelectionFrame | null,
): RotateFormState {
  if (frame === null || state.pivot === 'point') {
    return state;
  }
  const at = boxPivot(state.pivot, frame);
  return { ...state, pivotX: lengthText(at.x, units), pivotY: lengthText(at.y, units) };
}

export function validateRotate(state: RotateFormState, units: Units): RotateErrors {
  const errors: RotateErrors = {};
  const set = (field: RotateField, error: string | undefined) => {
    if (error !== undefined) {
      errors[field] = error;
    }
  };
  set('angle', fieldError(numberValue(state.angle)));
  if (state.pivot === 'point') {
    set('pivotX', fieldError(lengthValue(state.pivotX, units)));
    set('pivotY', fieldError(lengthValue(state.pivotY, units)));
  }
  return errors;
}

/** The rotation matrix for one step; null while invalid. */
export function rotateMatrixFromState(
  state: RotateFormState,
  units: Units,
  frame: SelectionFrame,
): Matrix | null {
  if (Object.keys(validateRotate(state, units)).length > 0) {
    return null;
  }
  const pivot =
    state.pivot === 'point'
      ? { x: lengthValue(state.pivotX, units) ?? 0, y: lengthValue(state.pivotY, units) ?? 0 }
      : boxPivot(state.pivot, frame);
  const typed = numberValue(state.angle) ?? 0;
  return rotateMatrix(pivot, state.mode === 'absolute' ? typed - frame.angle : typed);
}

export { snapPivot } from '../../lib/transformMath';

/** Fields with the pivot placed at a point. */
export function pivotAt(state: RotateFormState, units: Units, at: Point): RotateFormState {
  return {
    ...state,
    pivot: 'point',
    pivotX: lengthText(at.x, units),
    pivotY: lengthText(at.y, units),
  };
}
