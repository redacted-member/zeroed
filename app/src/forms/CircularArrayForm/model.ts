// Pure state for the Circular Copy form. The Total Angle is shared out
// between the items; a Step Angle is the angle from one item to the next.
// A positive angle places the copies clockwise.

import type { Bbox, CircularArrayArgs } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import { strings } from '../../lib/formStrings';

export type AngleMode = 'total' | 'step';

export interface CircularArrayFormState {
  count: string;
  angleMode: AngleMode;
  angle: string;
  /** Rotation center, filled from the selection's middle. */
  cx: string;
  cy: string;
  rotateCopies: boolean;
}

export type CircularArrayField = 'count' | 'angle' | 'cx' | 'cy';
export type CircularArrayErrors = Partial<Record<CircularArrayField, string>>;

export function initialCircularArrayState(units: Units): CircularArrayFormState {
  return {
    count: '6',
    angleMode: 'total',
    angle: '360',
    cx: lengthText(0, units),
    cy: lengthText(0, units),
    rotateCopies: true,
  };
}

export function convertCircularArrayUnits(
  state: CircularArrayFormState,
  from: Units,
  to: Units,
): CircularArrayFormState {
  return convertLengthFields(state, ['cx', 'cy'], from, to);
}

/** The fields with the rotation center at the middle of the selection. */
export function centreFromSelection(
  state: CircularArrayFormState,
  units: Units,
  selection: Bbox | null,
): CircularArrayFormState {
  if (selection === null) {
    return state;
  }
  return {
    ...state,
    cx: lengthText((selection[0] + selection[2]) / 2, units),
    cy: lengthText((selection[1] + selection[3]) / 2, units),
  };
}

export function validateCircularArray(
  state: CircularArrayFormState,
  units: Units,
): CircularArrayErrors {
  const errors: CircularArrayErrors = {};
  const count = fieldError(numberValue(state.count), { min: 2, max: 1000, integer: true });
  const value = numberValue(state.angle);
  const angle =
    fieldError(value, { min: -360, max: 360 }) ??
    (value === 0 ? strings.editObjects.circularArray.angleZero : undefined);
  const cx = fieldError(lengthValue(state.cx, units));
  const cy = fieldError(lengthValue(state.cy, units));
  if (count !== undefined) {
    errors.count = count;
  }
  if (angle !== undefined) {
    errors.angle = angle;
  }
  if (cx !== undefined) {
    errors.cx = cx;
  }
  if (cy !== undefined) {
    errors.cy = cy;
  }
  return errors;
}

/** Kernel arguments, or null while a field is invalid. */
export function circularArrayArgs(
  state: CircularArrayFormState,
  units: Units,
): CircularArrayArgs | null {
  if (Object.keys(validateCircularArray(state, units)).length > 0) {
    return null;
  }
  const count = numberValue(state.count) ?? 2;
  const angle = numberValue(state.angle) ?? 360;
  const clockwise = state.angleMode === 'total' ? angle / count : angle;
  return {
    cx: lengthValue(state.cx, units) ?? 0,
    cy: lengthValue(state.cy, units) ?? 0,
    count,
    angle,
    rotateCopies: state.rotateCopies,
    radius: null,
    group: false,
    step: clockwise === 0 ? 0 : -clockwise,
  };
}
