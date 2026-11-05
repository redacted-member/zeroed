// Pure form state for Draw Ellipse: anchor, position, width and height.

import type { Path, Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { boxOrigin, ellipse, type Anchor } from '../../lib/shapes';
import { anchorPoint } from '../DrawRectangleForm/model';
import { bboxAnchor } from '../../lib/transformMath';
import type { QuickEntry } from '../../lib/quickKeys';
import { boxCorner, quickBox } from '../../lib/quickShapes';
import { toMillimetres } from '../../lib/units';

export interface EllipseFormState {
  anchor: Anchor;
  x: string;
  y: string;
  width: string;
  height: string;
}

export type EllipseField = 'x' | 'y' | 'width' | 'height';
export type EllipseErrors = Partial<Record<EllipseField, string>>;

const LENGTH_FIELDS: readonly EllipseField[] = ['x', 'y', 'width', 'height'];

export function initialEllipseState(units: Units): EllipseFormState {
  return {
    anchor: 'centre',
    x: lengthText(0, units),
    y: lengthText(0, units),
    width: lengthText(60, units),
    height: lengthText(40, units),
  };
}

export function convertEllipseUnits(
  state: EllipseFormState,
  from: Units,
  to: Units,
): EllipseFormState {
  return convertLengthFields(state, LENGTH_FIELDS, from, to);
}

export function validateEllipse(state: EllipseFormState, units: Units): EllipseErrors {
  const errors: EllipseErrors = {};
  const rules: Array<[EllipseField, boolean]> = [
    ['x', false],
    ['y', false],
    ['width', true],
    ['height', true],
  ];
  for (const [field, positive] of rules) {
    const error = fieldError(lengthValue(state[field], units), { positive });
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

export function ellipsePathFromState(state: EllipseFormState, units: Units): Path | null {
  if (Object.keys(validateEllipse(state, units)).length > 0) {
    return null;
  }
  const width = lengthValue(state.width, units) ?? 0;
  const height = lengthValue(state.height, units) ?? 0;
  const origin = boxOrigin(
    state.anchor,
    lengthValue(state.x, units) ?? 0,
    lengthValue(state.y, units) ?? 0,
    width,
    height,
  );
  return ellipse(origin.x + width / 2, origin.y + height / 2, width, height);
}

/** Fields for a corner to corner drag of the bounding box; shift keeps a circle. */
export function ellipseFromDrag(
  state: EllipseFormState,
  units: Units,
  start: Point,
  end: Point,
  square: boolean,
  fromCentre = false,
): EllipseFormState {
  let dx = end.x - start.x;
  let dy = end.y - start.y;
  if (square) {
    const side = Math.max(Math.abs(dx), Math.abs(dy));
    dx = Math.sign(dx || 1) * side;
    dy = Math.sign(dy || 1) * side;
  }
  // From the centre (Alt) the drag reaches a corner, so the box is twice as big.
  const grow = fromCentre ? 2 : 1;
  const width = Math.abs(dx) * grow;
  const height = Math.abs(dy) * grow;
  const origin = fromCentre
    ? { x: start.x - Math.abs(dx), y: start.y - Math.abs(dy) }
    : { x: Math.min(start.x, start.x + dx), y: Math.min(start.y, start.y + dy) };
  const at = anchorPoint(state.anchor, origin, width, height);
  return {
    ...state,
    x: lengthText(at.x, units),
    y: lengthText(at.y, units),
    width: lengthText(width, units),
    height: lengthText(height, units),
  };
}

export function ellipseAt(state: EllipseFormState, units: Units, point: Point): EllipseFormState {
  return { ...state, x: lengthText(point.x, units), y: lengthText(point.y, units) };
}

/** Fields for Quick Keys typed while dragging from `start` towards `cursor`. */
export function ellipseFromQuick(
  state: EllipseFormState,
  units: Units,
  start: Point,
  cursor: Point,
  entry: QuickEntry,
): EllipseFormState | null {
  const current = { width: Math.abs(cursor.x - start.x), height: Math.abs(cursor.y - start.y) };
  const box = quickBox(entry, current, (v) => toMillimetres(v, units), false);
  if (box === null) {
    return null;
  }
  return ellipseFromDrag(
    state,
    units,
    start,
    boxCorner(start, cursor, box.width, box.height),
    false,
  );
}

/** The fields showing an existing ellipse, placed by the chosen anchor. */
export function ellipseFromShape(
  state: EllipseFormState,
  units: Units,
  shape: { box: [number, number, number, number] },
): EllipseFormState {
  const [x0, y0, x1, y1] = shape.box;
  return {
    ...ellipsePlacedOn(state, units, shape),
    width: lengthText(x1 - x0, units),
    height: lengthText(y1 - y0, units),
  };
}

/** The fields with the anchor moved to an existing ellipse's, the size kept. */
export function ellipsePlacedOn(
  state: EllipseFormState,
  units: Units,
  shape: { box: [number, number, number, number] },
): EllipseFormState {
  const at = bboxAnchor(shape.box, state.anchor);
  return { ...state, x: lengthText(at.x, units), y: lengthText(at.y, units) };
}
