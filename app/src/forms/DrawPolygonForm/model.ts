// Pure form state for Draw Polygon: centre, side count and radius or diameter.

import type { Path, Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import { polygon } from '../../lib/shapes';
import type { QuickEntry } from '../../lib/quickKeys';
import { quickRadial } from '../../lib/quickShapes';
import { toMillimetres } from '../../lib/units';

export type PolygonSizeBy = 'radius' | 'diameter';

export interface PolygonFormState {
  centreX: string;
  centreY: string;
  sides: string;
  /** The radius, or the diameter when `sizeBy` is 'diameter'. */
  radius: string;
  sizeBy: PolygonSizeBy;
}

export type PolygonField = 'centreX' | 'centreY' | 'sides' | 'radius';
export type PolygonErrors = Partial<Record<PolygonField, string>>;

const LENGTH_FIELDS: readonly PolygonField[] = ['centreX', 'centreY', 'radius'];
export const MIN_SIDES = 3;
export const MAX_SIDES = 100;

export function initialPolygonState(units: Units): PolygonFormState {
  return {
    centreX: lengthText(0, units),
    centreY: lengthText(0, units),
    sides: '6',
    radius: lengthText(25, units),
    sizeBy: 'radius',
  };
}

export function convertPolygonUnits(
  state: PolygonFormState,
  from: Units,
  to: Units,
): PolygonFormState {
  return convertLengthFields(state, LENGTH_FIELDS, from, to);
}

export function validatePolygon(state: PolygonFormState, units: Units): PolygonErrors {
  const errors: PolygonErrors = {};
  const x = fieldError(lengthValue(state.centreX, units));
  const y = fieldError(lengthValue(state.centreY, units));
  const sides = fieldError(numberValue(state.sides), {
    min: MIN_SIDES,
    max: MAX_SIDES,
    integer: true,
  });
  const radius = fieldError(lengthValue(state.radius, units), { positive: true });
  if (x !== undefined) {
    errors.centreX = x;
  }
  if (y !== undefined) {
    errors.centreY = y;
  }
  if (sides !== undefined) {
    errors.sides = sides;
  }
  if (radius !== undefined) {
    errors.radius = radius;
  }
  return errors;
}

/** The radius in millimetres the size field stands for, or null. */
function radiusOf(state: PolygonFormState, units: Units): number | null {
  const value = lengthValue(state.radius, units);
  return value === null ? null : state.sizeBy === 'diameter' ? value / 2 : value;
}

/** The size field showing a radius of `radius` millimetres. */
function sizeText(state: PolygonFormState, units: Units, radius: number): string {
  return lengthText(state.sizeBy === 'diameter' ? radius * 2 : radius, units);
}

/** Switches between radius and diameter, keeping the same polygon. */
export function switchPolygonSizeBy(
  state: PolygonFormState,
  units: Units,
  sizeBy: PolygonSizeBy,
): PolygonFormState {
  if (sizeBy === state.sizeBy) {
    return state;
  }
  const radius = radiusOf(state, units);
  const next = { ...state, sizeBy };
  return radius === null ? next : { ...next, radius: sizeText(next, units, radius) };
}

export function polygonPathFromState(state: PolygonFormState, units: Units): Path | null {
  if (Object.keys(validatePolygon(state, units)).length > 0) {
    return null;
  }
  return polygon(
    lengthValue(state.centreX, units) ?? 0,
    lengthValue(state.centreY, units) ?? 0,
    numberValue(state.sides) ?? MIN_SIDES,
    radiusOf(state, units) ?? 0,
  );
}

/** Fields for a drag from the centre to a corner; with `across` (Alt) the
 * drag spans the shape and the centre is its middle. */
export function polygonFromDrag(
  state: PolygonFormState,
  units: Units,
  start: Point,
  edge: Point,
  across = false,
): PolygonFormState {
  const centre = across ? { x: (start.x + edge.x) / 2, y: (start.y + edge.y) / 2 } : start;
  return {
    ...state,
    centreX: lengthText(centre.x, units),
    centreY: lengthText(centre.y, units),
    radius: sizeText(state, units, Math.hypot(edge.x - centre.x, edge.y - centre.y)),
  };
}

export function polygonAt(state: PolygonFormState, units: Units, centre: Point): PolygonFormState {
  return { ...state, centreX: lengthText(centre.x, units), centreY: lengthText(centre.y, units) };
}

/** Fields for Quick Keys typed while dragging from the centre. */
export function polygonFromQuick(
  state: PolygonFormState,
  units: Units,
  centre: Point,
  entry: QuickEntry,
): PolygonFormState | null {
  const radial = quickRadial(entry, (v) => toMillimetres(v, units), 's');
  if (radial === null) {
    return null;
  }
  const next = polygonFromDrag(state, units, centre, { x: centre.x + radial.radius, y: centre.y });
  return radial.count === undefined ? next : { ...next, sides: String(radial.count) };
}

/** The fields showing an existing regular polygon. */
export function polygonFromShape(
  state: PolygonFormState,
  units: Units,
  shape: { centre: Point; radius: number; sides: number },
): PolygonFormState {
  return {
    ...polygonAt(state, units, shape.centre),
    sides: String(shape.sides),
    radius: sizeText(state, units, shape.radius),
  };
}

/** The fields with the centre moved to an existing polygon's, the size kept. */
export function polygonCentredOn(
  state: PolygonFormState,
  units: Units,
  shape: { centre: Point },
): PolygonFormState {
  return polygonAt(state, units, shape.centre);
}
