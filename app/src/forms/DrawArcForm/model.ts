// Pure state for Draw Arc. Clicks place the arc through three points (start,
// end, a point on the arc) or by centre, start and end. The exact size
// fields describe the same arc by its start and end points and either its
// radius or its height (the rise of the arc above its chord); a negative
// value puts the arc on the other side of the chord. Clicked arcs fill the
// exact size fields, so they can be adjusted and created again.

import type { Path, Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { arcFromBulge } from '../../lib/geometry';
import { strings } from '../../lib/formStrings';
import { arcCentreStartEnd, arcThreePoints } from '../../lib/shapes';

export type ArcMode = 'threePoints' | 'centreStartEnd';
export type ArcSize = 'radius' | 'height';

export interface ArcFormState {
  mode: ArcMode;
  /** Points placed by clicking, in the order the mode expects. */
  placed: Point[];
  startX: string;
  startY: string;
  endX: string;
  endY: string;
  sizeBy: ArcSize;
  /** Radius or height; negative puts the arc on the other side. */
  size: string;
}

export type ArcField = 'startX' | 'startY' | 'endX' | 'endY' | 'size';
export type ArcErrors = Partial<Record<ArcField, string>>;

const FIELDS: readonly ArcField[] = ['startX', 'startY', 'endX', 'endY', 'size'];
const TINY = 1e-9;

export function initialArcState(units: Units): ArcFormState {
  return {
    mode: 'threePoints',
    placed: [],
    startX: lengthText(0, units),
    startY: lengthText(0, units),
    endX: lengthText(50, units),
    endY: lengthText(0, units),
    sizeBy: 'height',
    size: lengthText(-15, units),
  };
}

export function convertArcUnits(state: ArcFormState, from: Units, to: Units): ArcFormState {
  return convertLengthFields(state, FIELDS, from, to);
}

export function validateArc(state: ArcFormState, units: Units): ArcErrors {
  const errors: ArcErrors = {};
  for (const field of FIELDS) {
    const error = fieldError(lengthValue(state[field], units));
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  if (errors.size === undefined && exactArc(state, units) === null) {
    errors.size =
      state.sizeBy === 'radius'
        ? strings.createVectors.arc.radiusTooSmall
        : strings.createVectors.arc.heightZero;
  }
  return errors;
}

/** Builds the arc for three points in the mode's order, or null when degenerate. */
export function arcFromPoints(mode: ArcMode, a: Point, b: Point, c: Point): Path | null {
  if (mode === 'threePoints') {
    return arcThreePoints(a, c, b);
  }
  if (Math.hypot(b.x - a.x, b.y - a.y) < TINY) {
    return null;
  }
  return arcCentreStartEnd(a, b, c);
}

/**
 * The arc from `start` to `end` with the given radius or height. A positive
 * value turns counter clockwise from start to end; a radius gives the arc
 * shorter than a half circle.
 */
export function arcBetween(start: Point, end: Point, sizeBy: ArcSize, size: number): Path | null {
  const chord = Math.hypot(end.x - start.x, end.y - start.y);
  if (chord < TINY || Math.abs(size) < TINY) {
    return null;
  }
  let height = size;
  if (sizeBy === 'radius') {
    const r = Math.abs(size);
    const half = chord / 2;
    if (r + TINY < half) {
      return null;
    }
    height = Math.sign(size) * (r - Math.sqrt(Math.max(0, r * r - half * half)));
  }
  const bulge = (2 * height) / chord;
  return { subpaths: [{ start, closed: false, segments: [{ kind: 'arc', to: end, bulge }] }] };
}

/** The arc the exact size fields describe, or null while they do not make one. */
export function exactArc(state: ArcFormState, units: Units): Path | null {
  const value = (field: ArcField) => lengthValue(state[field], units);
  const sx = value('startX');
  const sy = value('startY');
  const ex = value('endX');
  const ey = value('endY');
  const size = value('size');
  if (sx === null || sy === null || ex === null || ey === null || size === null) {
    return null;
  }
  return arcBetween({ x: sx, y: sy }, { x: ex, y: ey }, state.sizeBy, size);
}

export function arcPathFromState(state: ArcFormState, units: Units): Path | null {
  return exactArc(state, units);
}

/** Writes an arc (from clicks) into the exact size fields. */
export function fillFromArc(state: ArcFormState, units: Units, path: Path): ArcFormState {
  const sub = path.subpaths[0];
  const segment = sub?.segments[0];
  if (sub === undefined || segment === undefined || segment.kind !== 'arc') {
    return state;
  }
  const chord = Math.hypot(segment.to.x - sub.start.x, segment.to.y - sub.start.y);
  const height = (segment.bulge * chord) / 2;
  const radius = arcFromBulge(sub.start, segment.to, segment.bulge)?.radius ?? 0;
  return {
    ...state,
    startX: lengthText(sub.start.x, units),
    startY: lengthText(sub.start.y, units),
    endX: lengthText(segment.to.x, units),
    endY: lengthText(segment.to.y, units),
    // A radius alone cannot describe an arc over a half circle.
    sizeBy: Math.abs(segment.bulge) > 1 ? 'height' : state.sizeBy,
    size: lengthText(
      Math.abs(segment.bulge) > 1 || state.sizeBy === 'height'
        ? height
        : Math.sign(segment.bulge) * radius,
      units,
    ),
  };
}

/** Switches between radius and height, keeping the same arc when there is one. */
export function withSizeBy(state: ArcFormState, units: Units, sizeBy: ArcSize): ArcFormState {
  if (sizeBy === state.sizeBy) {
    return state;
  }
  const path = exactArc(state, units);
  if (path === null) {
    return { ...state, sizeBy };
  }
  const sub = path.subpaths[0];
  const segment = sub?.segments[0];
  if (sub === undefined || segment === undefined || segment.kind !== 'arc') {
    return { ...state, sizeBy };
  }
  const chord = Math.hypot(segment.to.x - sub.start.x, segment.to.y - sub.start.y);
  const value =
    sizeBy === 'height'
      ? (segment.bulge * chord) / 2
      : Math.sign(segment.bulge) *
        (arcFromBulge(sub.start, segment.to, segment.bulge)?.radius ?? 0);
  return { ...state, sizeBy, size: lengthText(value, units) };
}

/** Places the next click. */
export function placePoint(state: ArcFormState, point: Point): ArcFormState {
  return { ...state, placed: [...state.placed, point].slice(0, 3) };
}

export function clearPlaced(state: ArcFormState): ArcFormState {
  return { ...state, placed: [] };
}

/** Preview while placing: the arc with the cursor standing in for the missing points. */
export function arcPreview(state: ArcFormState, cursor: Point | null): Path[] {
  const placed = state.placed;
  if (placed.length === 0 || cursor === null) {
    return [];
  }
  const a = placed[0] ?? cursor;
  if (placed.length === 1) {
    // Only a straight guide from the first point to the cursor.
    return [{ subpaths: [{ start: a, closed: false, segments: [{ kind: 'line', to: cursor }] }] }];
  }
  const b = placed[1] ?? cursor;
  const built = arcFromPoints(state.mode, a, b, cursor);
  return built === null ? [] : [built];
}

/** Radius of the arc in the path, for the read only field. */
export function arcRadiusOf(path: Path | null): number | null {
  const sub = path?.subpaths[0];
  const segment = sub?.segments[0];
  if (sub === undefined || segment === undefined || segment.kind !== 'arc') {
    return null;
  }
  return arcFromBulge(sub.start, segment.to, segment.bulge)?.radius ?? null;
}
