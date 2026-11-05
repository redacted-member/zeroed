// Pure state for Move Selection: anchor, absolute position or relative offset.
// The anchor is a point of the selection's frame, rotated with a rotated
// selection.

import type { Matrix, SelectionFrame } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import type { Anchor } from '../../lib/shapes';
import { translation } from '../../lib/geometry';
import { frameAnchor, type MoveMode } from '../../lib/transformMath';

export interface MoveFormState {
  anchor: Anchor;
  mode: MoveMode;
  x: string;
  y: string;
}

export type MoveField = 'x' | 'y';
export type MoveErrors = Partial<Record<MoveField, string>>;

export function initialMoveState(units: Units): MoveFormState {
  return { anchor: 'centre', mode: 'absolute', x: lengthText(0, units), y: lengthText(0, units) };
}

export function convertMoveUnits(state: MoveFormState, from: Units, to: Units): MoveFormState {
  return convertLengthFields(state, ['x', 'y'], from, to);
}

/** Fills the position fields from the selection's anchor (absolute mode). */
export function moveStateFromSelection(
  state: MoveFormState,
  units: Units,
  frame: SelectionFrame | null,
): MoveFormState {
  if (frame === null || state.mode === 'relative') {
    return state;
  }
  const at = frameAnchor(frame, state.anchor);
  return { ...state, x: lengthText(at.x, units), y: lengthText(at.y, units) };
}

export function switchMoveMode(
  state: MoveFormState,
  units: Units,
  mode: MoveMode,
  frame: SelectionFrame | null,
): MoveFormState {
  if (mode === state.mode) {
    return state;
  }
  const next = { ...state, mode };
  return mode === 'relative'
    ? { ...next, x: lengthText(0, units), y: lengthText(0, units) }
    : moveStateFromSelection(next, units, frame);
}

export function validateMove(state: MoveFormState, units: Units): MoveErrors {
  const errors: MoveErrors = {};
  const x = fieldError(lengthValue(state.x, units));
  const y = fieldError(lengthValue(state.y, units));
  if (x !== undefined) {
    errors.x = x;
  }
  if (y !== undefined) {
    errors.y = y;
  }
  return errors;
}

export function moveMatrixFromState(
  state: MoveFormState,
  units: Units,
  frame: SelectionFrame,
): Matrix | null {
  if (Object.keys(validateMove(state, units)).length > 0) {
    return null;
  }
  const x = lengthValue(state.x, units) ?? 0;
  const y = lengthValue(state.y, units) ?? 0;
  if (state.mode === 'relative') {
    return translation(x, y);
  }
  const at = frameAnchor(frame, state.anchor);
  return translation(x - at.x, y - at.y);
}
