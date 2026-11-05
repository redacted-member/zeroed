// Pure form state for Draw Rectangle: anchor, position, size and corners.

import type { Path, Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { boxOrigin, rectangle, type Anchor, type CornerType } from '../../lib/shapes';
import type { QuickEntry } from '../../lib/quickKeys';
import { boxCorner, quickBox } from '../../lib/quickShapes';
import { bboxAnchor } from '../../lib/transformMath';
import { toMillimetres } from '../../lib/units';

export interface RectangleFormState {
  anchor: Anchor;
  x: string;
  y: string;
  width: string;
  height: string;
  corner: CornerType;
  radius: string;
}

export type RectangleField = 'x' | 'y' | 'width' | 'height' | 'radius';
export type RectangleErrors = Partial<Record<RectangleField, string>>;

const LENGTH_FIELDS: readonly RectangleField[] = ['x', 'y', 'width', 'height', 'radius'];

export function initialRectangleState(units: Units): RectangleFormState {
  return {
    anchor: 'bottomLeft',
    x: lengthText(0, units),
    y: lengthText(0, units),
    width: lengthText(50, units),
    height: lengthText(30, units),
    corner: 'none',
    radius: lengthText(5, units),
  };
}

export function convertRectangleUnits(
  state: RectangleFormState,
  from: Units,
  to: Units,
): RectangleFormState {
  return convertLengthFields(state, LENGTH_FIELDS, from, to);
}

export function validateRectangle(state: RectangleFormState, units: Units): RectangleErrors {
  const errors: RectangleErrors = {};
  const width = lengthValue(state.width, units);
  const height = lengthValue(state.height, units);
  const check = (field: RectangleField, value: number | null, positive: boolean) => {
    const error = fieldError(value, { positive });
    if (error !== undefined) {
      errors[field] = error;
    }
  };
  check('x', lengthValue(state.x, units), false);
  check('y', lengthValue(state.y, units), false);
  check('width', width, true);
  check('height', height, true);
  if (state.corner !== 'none') {
    const radius = lengthValue(state.radius, units);
    check('radius', radius, true);
    if (radius !== null && width !== null && height !== null && errors.radius === undefined) {
      const limit = Math.min(width, height) / 2;
      if (radius > limit) {
        errors.radius = fieldError(radius, { max: limit });
      }
    }
  }
  return errors;
}

/** Path for the current fields, or null while they are invalid. */
export function rectanglePath(state: RectangleFormState, units: Units): Path | null {
  if (Object.keys(validateRectangle(state, units)).length > 0) {
    return null;
  }
  const x = lengthValue(state.x, units) ?? 0;
  const y = lengthValue(state.y, units) ?? 0;
  const width = lengthValue(state.width, units) ?? 0;
  const height = lengthValue(state.height, units) ?? 0;
  const radius = state.corner === 'none' ? 0 : (lengthValue(state.radius, units) ?? 0);
  const origin = boxOrigin(state.anchor, x, y, width, height);
  return rectangle(origin.x, origin.y, width, height, state.corner, radius);
}

/** Anchor position of a box, the inverse of boxOrigin. */
export function anchorPoint(anchor: Anchor, origin: Point, width: number, height: number): Point {
  const shifted = boxOrigin(anchor, 0, 0, width, height);
  return { x: origin.x - shifted.x, y: origin.y - shifted.y };
}

/** Fields for a corner to corner drag; shift keeps it square. */
export function rectangleFromDrag(
  state: RectangleFormState,
  units: Units,
  start: Point,
  end: Point,
  square: boolean,
  fromCentre = false,
): RectangleFormState {
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

/** Fields after a plain click: the anchor moves to the clicked point. */
export function rectangleAt(
  state: RectangleFormState,
  units: Units,
  point: Point,
): RectangleFormState {
  return { ...state, x: lengthText(point.x, units), y: lengthText(point.y, units) };
}

/** Fields for Quick Keys typed while dragging from `start` towards `cursor`. */
export function rectangleFromQuick(
  state: RectangleFormState,
  units: Units,
  start: Point,
  cursor: Point,
  entry: QuickEntry,
): RectangleFormState | null {
  const current = { width: Math.abs(cursor.x - start.x), height: Math.abs(cursor.y - start.y) };
  const box = quickBox(entry, current, (v) => toMillimetres(v, units), true);
  if (box === null) {
    return null;
  }
  const next = rectangleFromDrag(
    state,
    units,
    start,
    boxCorner(start, cursor, box.width, box.height),
    false,
  );
  if (box.radius === undefined) {
    return next;
  }
  return {
    ...next,
    corner: box.radius === 0 ? 'none' : state.corner === 'none' ? 'external' : state.corner,
    radius: lengthText(box.radius, units),
  };
}

/** The fields showing an existing rectangle, placed by the chosen anchor. */
export function rectangleFromShape(
  state: RectangleFormState,
  units: Units,
  shape: { box: [number, number, number, number]; corner: CornerType; radius: number },
): RectangleFormState {
  const [x0, y0, x1, y1] = shape.box;
  return {
    ...rectanglePlacedOn(state, units, shape),
    width: lengthText(x1 - x0, units),
    height: lengthText(y1 - y0, units),
    corner: shape.corner,
    radius: shape.corner === 'none' ? state.radius : lengthText(shape.radius, units),
  };
}

/** The fields with the anchor moved to an existing rectangle's, the size kept. */
export function rectanglePlacedOn(
  state: RectangleFormState,
  units: Units,
  shape: { box: [number, number, number, number] },
): RectangleFormState {
  const at = bboxAnchor(shape.box, state.anchor);
  return { ...state, x: lengthText(at.x, units), y: lengthText(at.y, units) };
}

/**
 * One of the four corner radius handles: it sits where the corner's arc meets
 * the top or bottom edge, and `inward` is the x direction the radius grows in.
 */
export interface RadiusHandle {
  at: Point;
  corner: Point;
  inward: 1 | -1;
}

/** The corner radius handles on the preview, none while the corners are square or invalid. */
export function radiusHandles(state: RectangleFormState, units: Units): RadiusHandle[] {
  if (state.corner === 'none' || rectanglePath(state, units) === null) {
    return [];
  }
  const width = lengthValue(state.width, units) ?? 0;
  const height = lengthValue(state.height, units) ?? 0;
  const r = lengthValue(state.radius, units) ?? 0;
  const origin = boxOrigin(
    state.anchor,
    lengthValue(state.x, units) ?? 0,
    lengthValue(state.y, units) ?? 0,
    width,
    height,
  );
  const x0 = origin.x;
  const y0 = origin.y;
  const x1 = x0 + width;
  const y1 = y0 + height;
  const handle = (cx: number, cy: number, inward: 1 | -1): RadiusHandle => ({
    at: { x: cx + inward * r, y: cy },
    corner: { x: cx, y: cy },
    inward,
  });
  return [handle(x0, y0, 1), handle(x1, y0, -1), handle(x1, y1, -1), handle(x0, y1, 1)];
}

/** The smallest radius a handle drag leaves, millimetres. */
const MIN_DRAG_RADIUS_MM = 0.01;

/** Fields with the radius a corner handle was dragged to, kept inside the box. */
export function radiusFromDrag(
  state: RectangleFormState,
  units: Units,
  handle: RadiusHandle,
  to: Point,
): RectangleFormState {
  const width = lengthValue(state.width, units) ?? 0;
  const height = lengthValue(state.height, units) ?? 0;
  const limit = Math.min(width, height) / 2;
  const along = (to.x - handle.corner.x) * handle.inward;
  const radius = Math.max(MIN_DRAG_RADIUS_MM, Math.min(limit, along));
  return { ...state, radius: lengthText(radius, units) };
}
