// Pure form state for Draw Star: centre, point count, outer and inner radius.

import type { Path, Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import { star } from '../../lib/shapes';
import type { QuickEntry } from '../../lib/quickKeys';
import { quickRadial } from '../../lib/quickShapes';
import { toMillimetres } from '../../lib/units';

export interface StarFormState {
  centreX: string;
  centreY: string;
  points: string;
  outerRadius: string;
  /** Inner radius as a percentage of the outer radius. */
  inner: string;
}

export type StarField = 'centreX' | 'centreY' | 'points' | 'outerRadius' | 'inner';
export type StarErrors = Partial<Record<StarField, string>>;

const LENGTH_FIELDS: readonly StarField[] = ['centreX', 'centreY', 'outerRadius'];
export const MIN_POINTS = 3;
export const MAX_POINTS = 100;

export function initialStarState(units: Units): StarFormState {
  return {
    centreX: lengthText(0, units),
    centreY: lengthText(0, units),
    points: '5',
    outerRadius: lengthText(25, units),
    inner: '40',
  };
}

export function convertStarUnits(state: StarFormState, from: Units, to: Units): StarFormState {
  return convertLengthFields(state, LENGTH_FIELDS, from, to);
}

/** Inner radius in millimetres. */
export function innerRadius(state: StarFormState, units: Units): number | null {
  const outer = lengthValue(state.outerRadius, units);
  const percent = numberValue(state.inner);
  return outer === null || percent === null ? null : (outer * percent) / 100;
}

export function validateStar(state: StarFormState, units: Units): StarErrors {
  const errors: StarErrors = {};
  const set = (field: StarField, error: string | undefined) => {
    if (error !== undefined) {
      errors[field] = error;
    }
  };
  set('centreX', fieldError(lengthValue(state.centreX, units)));
  set('centreY', fieldError(lengthValue(state.centreY, units)));
  set(
    'points',
    fieldError(numberValue(state.points), { min: MIN_POINTS, max: MAX_POINTS, integer: true }),
  );
  const outer = lengthValue(state.outerRadius, units);
  set('outerRadius', fieldError(outer, { positive: true }));
  set('inner', fieldError(numberValue(state.inner), { min: 1, max: 99 }));
  return errors;
}

export function starPathFromState(state: StarFormState, units: Units): Path | null {
  if (Object.keys(validateStar(state, units)).length > 0) {
    return null;
  }
  return star(
    lengthValue(state.centreX, units) ?? 0,
    lengthValue(state.centreY, units) ?? 0,
    numberValue(state.points) ?? MIN_POINTS,
    lengthValue(state.outerRadius, units) ?? 0,
    innerRadius(state, units) ?? 0,
  );
}

/** Fields for a drag from the centre to a point; with `across` (Alt) the
 * drag spans the star and the centre is its middle. */
export function starFromDrag(
  state: StarFormState,
  units: Units,
  start: Point,
  edge: Point,
  across = false,
): StarFormState {
  const centre = across ? { x: (start.x + edge.x) / 2, y: (start.y + edge.y) / 2 } : start;
  const outer = Math.hypot(edge.x - centre.x, edge.y - centre.y);
  return {
    ...state,
    centreX: lengthText(centre.x, units),
    centreY: lengthText(centre.y, units),
    outerRadius: lengthText(outer, units),
  };
}

export function starAt(state: StarFormState, units: Units, centre: Point): StarFormState {
  return { ...state, centreX: lengthText(centre.x, units), centreY: lengthText(centre.y, units) };
}

/** Fields for Quick Keys typed while dragging from the centre. */
export function starFromQuick(
  state: StarFormState,
  units: Units,
  centre: Point,
  entry: QuickEntry,
): StarFormState | null {
  const radial = quickRadial(entry, (v) => toMillimetres(v, units), 'p');
  if (radial === null) {
    return null;
  }
  let next = starFromDrag(state, units, centre, { x: centre.x + radial.radius, y: centre.y });
  if (radial.count !== undefined) {
    next = { ...next, points: String(radial.count) };
  }
  if (radial.innerPercent !== undefined) {
    next = { ...next, inner: String(radial.innerPercent) };
  }
  return next;
}

/** The fields showing an existing star. */
export function starFromShape(
  state: StarFormState,
  units: Units,
  shape: { centre: Point; points: number; outer: number; inner: number },
): StarFormState {
  return {
    ...starAt(state, units, shape.centre),
    points: String(shape.points),
    outerRadius: lengthText(shape.outer, units),
    inner: String(Math.round((shape.inner / shape.outer) * 1000) / 10),
  };
}

/** The fields with the centre moved to an existing star's, the size kept. */
export function starCentredOn(
  state: StarFormState,
  units: Units,
  shape: { centre: Point },
): StarFormState {
  return starAt(state, units, shape.centre);
}
