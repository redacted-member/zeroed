// Pure state for Draw Line / Polyline: the points placed so far, the typed
// next point, and the rules for closing, finishing and angle constraints.
// Clicking adds straight spans; clicking and dragging makes the span a
// smooth curve and keeps curving later spans until S turns it off. An open
// vector can be carried on from one of its ends.

import type { Path, Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import { spanSegments, type Circle } from '../../lib/spans';
import { polar, snapAngle } from '../../lib/shapes';

export interface PolylineFormState {
  points: Point[];
  /** Per span from points[i] to points[i + 1]: a smooth curve, not a line. */
  curves: boolean[];
  /** Per point: the unit direction a click and drag gave it, or null. */
  tangents: Array<Point | null>;
  /** New spans are curves while this is on; a drag turns it on, S toggles it. */
  curveMode: boolean;
  /** The typed next point is an offset from the last point. */
  relative: boolean;
  /** The open vector being carried on, and from which end. */
  extend: { id: number; atEnd: boolean } | null;
  nextX: string;
  nextY: string;
  length: string;
  angle: string;
}

export type PolylineField = 'nextX' | 'nextY' | 'length' | 'angle';
export type PolylineErrors = Partial<Record<PolylineField, string>>;

export const ANGLE_STEP_DEG = 15;

/** A fresh line; `curveMode` is the mode last used (a preference). */
export function initialPolylineState(units: Units, curveMode = false): PolylineFormState {
  return {
    points: [],
    curves: [],
    tangents: [],
    curveMode,
    relative: false,
    extend: null,
    nextX: lengthText(0, units),
    nextY: lengthText(0, units),
    length: lengthText(10, units),
    angle: '0',
  };
}

export function convertPolylineUnits(
  state: PolylineFormState,
  from: Units,
  to: Units,
): PolylineFormState {
  return convertLengthFields(state, ['nextX', 'nextY', 'length'], from, to);
}

export function validatePolyline(state: PolylineFormState, units: Units): PolylineErrors {
  const errors: PolylineErrors = {};
  const set = (field: PolylineField, error: string | undefined) => {
    if (error !== undefined) {
      errors[field] = error;
    }
  };
  set('nextX', fieldError(lengthValue(state.nextX, units)));
  set('nextY', fieldError(lengthValue(state.nextY, units)));
  set('length', fieldError(lengthValue(state.length, units), { positive: true }));
  set('angle', fieldError(numberValue(state.angle)));
  return errors;
}

export function lastPoint(state: PolylineFormState): Point | null {
  return state.points[state.points.length - 1] ?? null;
}

const DUPLICATE_TOLERANCE = 1e-6;

/**
 * Appends a point; the typed fields follow it (absolute entry only). A
 * click on the last point switches curve mode instead. `drag` is the drag vector of a click
 * and drag, which makes the new span a curve leaving along it.
 */
export function addPoint(
  state: PolylineFormState,
  units: Units,
  point: Point,
  drag?: Point,
): PolylineFormState {
  const last = lastPoint(state);
  if (last !== null && Math.hypot(point.x - last.x, point.y - last.y) <= DUPLICATE_TOLERANCE) {
    // A click on the last point switches between curves and straight lines.
    return toggleCurveMode(state);
  }
  const dragged = drag !== undefined && Math.hypot(drag.x, drag.y) > DUPLICATE_TOLERANCE;
  const curveMode = dragged || state.curveMode;
  const length = dragged ? Math.hypot(drag.x, drag.y) : 1;
  const tangent = dragged ? { x: drag.x / length, y: drag.y / length } : null;
  return {
    ...state,
    points: [...state.points, point],
    curves: last === null ? state.curves : [...state.curves, curveMode],
    tangents: [...state.tangents, tangent],
    curveMode,
    ...(state.relative
      ? {}
      : { nextX: lengthText(point.x, units), nextY: lengthText(point.y, units) }),
  };
}

/** S: new spans switch between curves and straight lines. */
export function toggleCurveMode(state: PolylineFormState): PolylineFormState {
  return { ...state, curveMode: !state.curveMode };
}

/** Switches the typed point between absolute and relative to the last point. */
export function setRelative(
  state: PolylineFormState,
  units: Units,
  relative: boolean,
): PolylineFormState {
  if (relative === state.relative) {
    return state;
  }
  const last = lastPoint(state);
  return relative
    ? { ...state, relative, nextX: lengthText(0, units), nextY: lengthText(0, units) }
    : {
        ...state,
        relative,
        nextX: lengthText(last?.x ?? 0, units),
        nextY: lengthText(last?.y ?? 0, units),
      };
}

export function undoPoint(state: PolylineFormState): PolylineFormState {
  const points = state.points.slice(0, -1);
  return {
    ...state,
    points,
    curves: state.curves.slice(0, Math.max(0, points.length - 1)),
    tangents: state.tangents.slice(0, points.length),
    extend: points.length === 0 ? null : state.extend,
  };
}

/** Clears the line; curve mode is remembered for the next one. */
export function clearPoints(state: PolylineFormState): PolylineFormState {
  return { ...state, points: [], curves: [], tangents: [], extend: null };
}

/** Starts a line on the end of an open vector, to carry that vector on. */
export function startExtending(
  state: PolylineFormState,
  units: Units,
  at: Point,
  extend: { id: number; atEnd: boolean },
): PolylineFormState {
  return { ...addPoint(clearPoints(state), units, at), extend };
}

/** The typed point (absolute, or offset from the last point), or null while invalid. */
export function typedPoint(state: PolylineFormState, units: Units): Point | null {
  const x = lengthValue(state.nextX, units);
  const y = lengthValue(state.nextY, units);
  if (x === null || y === null) {
    return null;
  }
  const last = lastPoint(state);
  return state.relative && last !== null ? { x: last.x + x, y: last.y + y } : { x, y };
}

/** The point at the typed length and angle from the last point. */
export function polarPoint(state: PolylineFormState, units: Units): Point | null {
  const from = lastPoint(state);
  const length = lengthValue(state.length, units);
  const angle = numberValue(state.angle);
  if (from === null || length === null || length <= 0 || angle === null) {
    return null;
  }
  return polar(from, length, angle);
}

/** Constrains `to` to a multiple of the angle step around `from`. */
export function constrainAngle(from: Point, to: Point): Point {
  const length = Math.hypot(to.x - from.x, to.y - from.y);
  const angle = (Math.atan2(to.y - from.y, to.x - from.x) * 180) / Math.PI;
  return polar(from, length, snapAngle(angle, ANGLE_STEP_DEG));
}

/** True when the point is within tolerance of the first point and the shape can close. */
export function closesShape(state: PolylineFormState, point: Point, tolerance: number): boolean {
  const first = state.points[0];
  return (
    state.points.length >= 3 &&
    first !== undefined &&
    Math.hypot(point.x - first.x, point.y - first.y) <= tolerance
  );
}

/** Path for the placed points, or null with fewer than two (three when closed). */
export function polylinePathFromState(state: PolylineFormState, closed: boolean): Path | null {
  if (state.points.length < 2) {
    return null;
  }
  if (closed && state.points.length < 3) {
    return null;
  }
  const sub = spanSegments(state.points, state.curves, state.tangents, closed, state.curveMode);
  return sub === null ? null : { subpaths: [sub] };
}

/** Preview: the placed points plus a rubber span to the cursor. */
export function polylinePreview(state: PolylineFormState, cursor: Point | null): Path[] {
  const points = cursor === null ? state.points : [...state.points, cursor];
  if (points.length < 2) {
    return [];
  }
  const curves = cursor === null ? state.curves : [...state.curves, state.curveMode];
  const sub = spanSegments(points, curves, [...state.tangents, null], false);
  return sub === null ? [] : [{ subpaths: [sub] }];
}

/**
 * T: the point where a line from `from` touches `circle`, on the side
 * nearer `near`; null when `from` is inside the circle.
 */
export function tangentPoint(from: Point, circle: Circle, near: Point): Point | null {
  const dx = from.x - circle.centre.x;
  const dy = from.y - circle.centre.y;
  const d = Math.hypot(dx, dy);
  if (d <= circle.radius + DUPLICATE_TOLERANCE) {
    return null;
  }
  const theta = Math.atan2(dy, dx);
  const alpha = Math.acos(circle.radius / d);
  const touch = (angle: number): Point => ({
    x: circle.centre.x + circle.radius * Math.cos(angle),
    y: circle.centre.y + circle.radius * Math.sin(angle),
  });
  const a = touch(theta + alpha);
  const b = touch(theta - alpha);
  return Math.hypot(a.x - near.x, a.y - near.y) <= Math.hypot(b.x - near.x, b.y - near.y) ? a : b;
}

/** Moves the last point (a tangent starting on an arc slides it along the arc). */
export function moveLastPoint(
  state: PolylineFormState,
  units: Units,
  point: Point,
): PolylineFormState {
  if (state.points.length === 0) {
    return state;
  }
  return {
    ...state,
    points: [...state.points.slice(0, -1), point],
    ...(state.relative
      ? {}
      : { nextX: lengthText(point.x, units), nextY: lengthText(point.y, units) }),
  };
}

/** Whether a point lies on a circle, within `tolerance` millimetres. */
export function onCircle(point: Point, circle: Circle, tolerance: number): boolean {
  const d = Math.hypot(point.x - circle.centre.x, point.y - circle.centre.y);
  return Math.abs(d - circle.radius) <= tolerance;
}

/**
 * T from an arc to an arc: the line touching both circles (outer or crossing)
 * whose touch points lie nearest `nearA` on `a` and `nearB` on `b`; null
 * when one circle lies inside the other.
 */
export function commonTangent(
  a: Circle,
  b: Circle,
  nearA: Point,
  nearB: Point,
): [Point, Point] | null {
  const dx = b.centre.x - a.centre.x;
  const dy = b.centre.y - a.centre.y;
  const d = Math.hypot(dx, dy);
  if (d <= DUPLICATE_TOLERANCE) {
    return null;
  }
  const base = Math.atan2(dy, dx);
  let best: { pair: [Point, Point]; cost: number } | null = null;
  // Outer tangents keep both circles on one side; crossing ones pass between.
  for (const side of [1, -1]) {
    const cos = (a.radius - side * b.radius) / d;
    if (Math.abs(cos) > 1) {
      continue;
    }
    const spread = Math.acos(cos);
    for (const angle of [base + spread, base - spread]) {
      const nx = Math.cos(angle);
      const ny = Math.sin(angle);
      const pa = { x: a.centre.x + a.radius * nx, y: a.centre.y + a.radius * ny };
      const pb = {
        x: b.centre.x + side * b.radius * nx,
        y: b.centre.y + side * b.radius * ny,
      };
      if (Math.hypot(pb.x - pa.x, pb.y - pa.y) <= DUPLICATE_TOLERANCE) {
        continue;
      }
      const cost =
        Math.hypot(pa.x - nearA.x, pa.y - nearA.y) + Math.hypot(pb.x - nearB.x, pb.y - nearB.y);
      if (best === null || cost < best.cost) {
        best = { pair: [pa, pb], cost };
      }
    }
  }
  return best?.pair ?? null;
}

/** What a key does to the line being drawn: Space ends it open and Tab
 * closes it. Enter only takes typed Quick Keys and a double click adds no
 * ending of its own. */
export function lineEndKey(key: string): 'open' | 'close' | null {
  if (key === ' ') {
    return 'open';
  }
  return key === 'Tab' ? 'close' : null;
}
