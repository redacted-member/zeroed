// Pure form state for Draw Circle: centre and radius or diameter.

import type { Path, Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { circle } from '../../lib/shapes';
import type { QuickEntry } from '../../lib/quickKeys';
import { quickCircleRadius } from '../../lib/quickShapes';
import { toMillimetres } from '../../lib/units';

export type CircleSizeBy = 'radius' | 'diameter';

export interface CircleFormState {
  centreX: string;
  centreY: string;
  sizeBy: CircleSizeBy;
  size: string;
}

export type CircleField = 'centreX' | 'centreY' | 'size';
export type CircleErrors = Partial<Record<CircleField, string>>;

const LENGTH_FIELDS: readonly CircleField[] = ['centreX', 'centreY', 'size'];

export function initialCircleState(units: Units): CircleFormState {
  return {
    centreX: lengthText(0, units),
    centreY: lengthText(0, units),
    sizeBy: 'radius',
    size: lengthText(25, units),
  };
}

export function convertCircleUnits(
  state: CircleFormState,
  from: Units,
  to: Units,
): CircleFormState {
  return convertLengthFields(state, LENGTH_FIELDS, from, to);
}

export function validateCircle(state: CircleFormState, units: Units): CircleErrors {
  const errors: CircleErrors = {};
  const x = fieldError(lengthValue(state.centreX, units));
  const y = fieldError(lengthValue(state.centreY, units));
  const size = fieldError(lengthValue(state.size, units), { positive: true });
  if (x !== undefined) {
    errors.centreX = x;
  }
  if (y !== undefined) {
    errors.centreY = y;
  }
  if (size !== undefined) {
    errors.size = size;
  }
  return errors;
}

export function circleRadius(state: CircleFormState, units: Units): number | null {
  const size = lengthValue(state.size, units);
  if (size === null || size <= 0) {
    return null;
  }
  return state.sizeBy === 'radius' ? size : size / 2;
}

export function circlePathFromState(state: CircleFormState, units: Units): Path | null {
  if (Object.keys(validateCircle(state, units)).length > 0) {
    return null;
  }
  const radius = circleRadius(state, units) ?? 0;
  return circle(
    lengthValue(state.centreX, units) ?? 0,
    lengthValue(state.centreY, units) ?? 0,
    radius,
  );
}

/** Switches between radius and diameter, keeping the same circle. */
export function switchSizeBy(
  state: CircleFormState,
  units: Units,
  sizeBy: CircleSizeBy,
): CircleFormState {
  if (sizeBy === state.sizeBy) {
    return state;
  }
  const value = lengthValue(state.size, units);
  const converted =
    value === null ? state.size : lengthText(sizeBy === 'radius' ? value / 2 : value * 2, units);
  return { ...state, sizeBy, size: converted };
}

/**
 * Fields for a drag from the centre out to the radius, or with `across`
 * (Alt) from one side of the circle to the other, its centre at the middle.
 */
export function circleFromDrag(
  state: CircleFormState,
  units: Units,
  start: Point,
  edge: Point,
  across = false,
): CircleFormState {
  const centre = across ? { x: (start.x + edge.x) / 2, y: (start.y + edge.y) / 2 } : start;
  const radius = Math.hypot(edge.x - centre.x, edge.y - centre.y);
  return {
    ...state,
    centreX: lengthText(centre.x, units),
    centreY: lengthText(centre.y, units),
    size: lengthText(state.sizeBy === 'radius' ? radius : radius * 2, units),
  };
}

export function circleAt(state: CircleFormState, units: Units, centre: Point): CircleFormState {
  return { ...state, centreX: lengthText(centre.x, units), centreY: lengthText(centre.y, units) };
}

/** Fields for Quick Keys typed while dragging from the centre. */
export function circleFromQuick(
  state: CircleFormState,
  units: Units,
  centre: Point,
  entry: QuickEntry,
): CircleFormState | null {
  const radius = quickCircleRadius(entry, (v) => toMillimetres(v, units));
  if (radius === null) {
    return null;
  }
  return circleFromDrag(state, units, centre, { x: centre.x + radius, y: centre.y });
}

/** Fields showing an existing circle. */
export function circleFromShape(
  state: CircleFormState,
  units: Units,
  shape: { centre: Point; radius: number },
): CircleFormState {
  return {
    ...state,
    centreX: lengthText(shape.centre.x, units),
    centreY: lengthText(shape.centre.y, units),
    size: lengthText(state.sizeBy === 'radius' ? shape.radius : shape.radius * 2, units),
  };
}

/** The fields with the centre moved to an existing circle's, the size kept. */
export function circleCentredOn(
  state: CircleFormState,
  units: Units,
  shape: { centre: Point },
): CircleFormState {
  return circleAt(state, units, shape.centre);
}
