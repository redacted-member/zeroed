// Pure state for Draw Dimensions: the points a dimension is measured from,
// placed by clicks or typed, and the kernel spec they make.

import type { DimensionSpec, Path, Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';

/** The types: Length, Vertical, Horizontal, Angle and Arc. */
export type DimensionKind = 'linear' | 'vertical' | 'horizontal' | 'angular' | 'arc';

export type DimensionField = 'aX' | 'aY' | 'bX' | 'bY' | 'cX' | 'cY' | 'dX' | 'dY' | 'eX' | 'eY';

export interface DimensionFormState {
  kind: DimensionKind;
  /** Points placed by clicking, in the order the kind asks for them. */
  placed: Point[];
  aX: string;
  aY: string;
  bX: string;
  bY: string;
  cX: string;
  cY: string;
  dX: string;
  dY: string;
  eX: string;
  eY: string;
  /** Radius of the picked arc or circle, for an Arc dimension. */
  radius: string;
  /** Show Diameter: an Arc dimension reads the diameter. */
  showDiameter: boolean;
  /** Use Custom Text: show `customText` in place of the measured value. */
  useCustom: boolean;
  /** Text shown in place of the measured value; empty shows the value. */
  customText: string;
}

export type DimensionErrors = Partial<Record<DimensionField | 'radius', string>>;

const FIELDS: readonly DimensionField[] = [
  'aX',
  'aY',
  'bX',
  'bY',
  'cX',
  'cY',
  'dX',
  'dY',
  'eX',
  'eY',
];
export const PAIRS: ReadonlyArray<[DimensionField, DimensionField]> = [
  ['aX', 'aY'],
  ['bX', 'bY'],
  ['cX', 'cY'],
  ['dX', 'dY'],
  ['eX', 'eY'],
];

/** How many points each kind needs; the last one places the text. */
export function pointCount(kind: DimensionKind): number {
  if (kind === 'angular') {
    return 5;
  }
  if (kind === 'arc') {
    return 2;
  }
  return 4;
}

/** Labels of the points, in placing order. */
export function pointLabels(kind: DimensionKind): string[] {
  const text = strings.createVectors.dimensions;
  switch (kind) {
    case 'angular':
      return [
        text.pointFirstArm,
        text.pointCorner,
        text.pointSecondArm,
        text.pointArc,
        text.pointText,
      ];
    case 'arc':
      return [text.pointCentre, text.pointText];
    default:
      return [text.pointFrom, text.pointTo, text.pointLine, text.pointText];
  }
}

export function initialDimensionState(units: Units): DimensionFormState {
  return {
    kind: 'linear',
    placed: [],
    aX: lengthText(0, units),
    aY: lengthText(0, units),
    bX: lengthText(50, units),
    bY: lengthText(0, units),
    cX: lengthText(25, units),
    cY: lengthText(10, units),
    dX: lengthText(25, units),
    dY: lengthText(10, units),
    eX: lengthText(25, units),
    eY: lengthText(12, units),
    radius: lengthText(10, units),
    showDiameter: false,
    useCustom: false,
    customText: '',
  };
}

/** The custom text a new or edited dimension shows, or undefined for its value. */
export function customTextFor(state: DimensionFormState): string | undefined {
  const text = state.customText.trim();
  return state.useCustom && text !== '' ? text : undefined;
}

export function convertDimensionUnits(
  state: DimensionFormState,
  from: Units,
  to: Units,
): DimensionFormState {
  return convertLengthFields(state, [...FIELDS, 'radius'], from, to);
}

export function validateDimension(state: DimensionFormState, units: Units): DimensionErrors {
  const errors: DimensionErrors = {};
  for (const [x, y] of PAIRS.slice(0, pointCount(state.kind))) {
    for (const field of [x, y]) {
      const error = fieldError(lengthValue(state[field], units));
      if (error !== undefined) {
        errors[field] = error;
      }
    }
  }
  if (state.kind === 'arc') {
    const error = fieldError(lengthValue(state.radius, units), { positive: true });
    if (error !== undefined) {
      errors.radius = error;
    }
  }
  return errors;
}

/** The dimension a kind makes from its points, or null when they cannot make one. */
export function specFromPoints(
  kind: DimensionKind,
  points: readonly Point[],
  radius = 0,
  showDiameter = false,
): DimensionSpec | null {
  const [a, b, c, d] = points;
  if (a === undefined || b === undefined) {
    return null;
  }
  if (kind === 'arc') {
    return radius > 0 ? { kind: 'radius', centre: a, radius, at: b, diameter: showDiameter } : null;
  }
  if (c === undefined) {
    return null;
  }
  switch (kind) {
    case 'horizontal':
      return Math.abs(b.x - a.x) < 1e-9 ? null : { kind, from: a, to: b, offset: c.y };
    case 'vertical':
      return Math.abs(b.y - a.y) < 1e-9 ? null : { kind, from: a, to: b, offset: c.x };
    case 'linear': {
      const length = Math.hypot(b.x - a.x, b.y - a.y);
      if (length < 1e-9) {
        return null;
      }
      // Positive to the left of the direction from the first point to the second.
      const offset = ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)) / length;
      return { kind, from: a, to: b, offset };
    }
    case 'angular': {
      // First arm, the corner, the second arm, then the line's radius.
      if (d === undefined) {
        return null;
      }
      const radius = Math.hypot(d.x - b.x, d.y - b.y);
      const armsOk =
        Math.hypot(a.x - b.x, a.y - b.y) > 1e-9 && Math.hypot(c.x - b.x, c.y - b.y) > 1e-9;
      return radius < 1e-9 || !armsOk ? null : { kind, centre: b, from: a, to: c, radius };
    }
  }
}

/** Where the final click put the text of a Length, Horizontal, Vertical
 * or Angle dimension; an Arc carries its text point in the spec. */
export function textAtOf(kind: DimensionKind, points: readonly Point[]): Point | undefined {
  switch (kind) {
    case 'angular':
      return points[4];
    case 'arc':
      return undefined;
    default:
      return points[3];
  }
}

/** The points that make `spec` again, in placing order: for a Length,
 * Horizontal or Vertical dimension the two measured points, a point on the
 * dimension line and the text; for an Angle the first arm, the corner, the
 * second arm, a point on the arc and the text; for an Arc the center and
 * the text. `textAt` is where the text was dragged, the middle when absent. */
export function pointsOfDimension(
  spec: DimensionSpec,
  textAt: Point | undefined,
): { kind: DimensionKind; points: Point[] } | null {
  switch (spec.kind) {
    case 'linear':
    case 'horizontal':
    case 'vertical': {
      const { from, to } = spec;
      const mid = { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 };
      let line: Point;
      if (spec.kind === 'horizontal') {
        line = { x: mid.x, y: spec.offset };
      } else if (spec.kind === 'vertical') {
        line = { x: spec.offset, y: mid.y };
      } else {
        const length = Math.hypot(to.x - from.x, to.y - from.y);
        const nx = length < 1e-9 ? 0 : -(to.y - from.y) / length;
        const ny = length < 1e-9 ? 0 : (to.x - from.x) / length;
        line = { x: mid.x + nx * spec.offset, y: mid.y + ny * spec.offset };
      }
      return { kind: spec.kind, points: [from, to, line, textAt ?? line] };
    }
    case 'angular': {
      const { centre, from, to, radius } = spec;
      const arm = Math.hypot(from.x - centre.x, from.y - centre.y);
      const onArc =
        arm < 1e-9
          ? { x: centre.x + radius, y: centre.y }
          : {
              x: centre.x + ((from.x - centre.x) / arm) * radius,
              y: centre.y + ((from.y - centre.y) / arm) * radius,
            };
      return { kind: 'angular', points: [from, centre, to, onArc, textAt ?? onArc] };
    }
    case 'radius':
      return { kind: 'arc', points: [spec.centre, spec.at] };
    case 'note':
      return null;
  }
}

/** The form set up to edit a dimension picked with Shift: its type, its
 * points, its settings and its own text; null for a dimension the form
 * cannot make (a note from an older job). */
export function stateFromDimension(
  state: DimensionFormState,
  units: Units,
  spec: DimensionSpec,
  customText: string,
  textAt?: Point,
): DimensionFormState | null {
  const loaded = pointsOfDimension(spec, textAt);
  if (loaded === null) {
    return null;
  }
  const next: DimensionFormState = {
    ...clearPlaced(state),
    kind: loaded.kind,
    showDiameter: spec.kind === 'radius' ? spec.diameter === true : state.showDiameter,
    radius: spec.kind === 'radius' ? lengthText(spec.radius, units) : state.radius,
    useCustom: customText !== '',
    customText,
  };
  loaded.points.forEach((p, index) => {
    const pair = PAIRS[index];
    if (pair !== undefined) {
      next[pair[0]] = lengthText(p.x, units);
      next[pair[1]] = lengthText(p.y, units);
    }
  });
  return next;
}

/**
 * Where an edited dimension's text goes on Apply: where it was (the middle
 * when it was never dragged) while its text point is as loaded, else the
 * typed text point. An Arc carries its text point in the spec.
 */
export function editedTextAt(
  state: DimensionFormState,
  units: Units,
  loaded: DimensionFormState,
  textAt: Point | undefined,
): Point | null {
  if (state.kind === 'arc') {
    return null;
  }
  const pair = PAIRS[pointCount(state.kind) - 1];
  const unchanged =
    pair !== undefined &&
    loaded.kind === state.kind &&
    state[pair[0]] === loaded[pair[0]] &&
    state[pair[1]] === loaded[pair[1]];
  if (unchanged) {
    return textAt ?? null;
  }
  return textAtOf(state.kind, typedPoints(state, units) ?? []) ?? null;
}

export function typedPoints(state: DimensionFormState, units: Units): Point[] | null {
  const points: Point[] = [];
  for (const [x, y] of PAIRS.slice(0, pointCount(state.kind))) {
    const px = lengthValue(state[x], units);
    const py = lengthValue(state[y], units);
    if (px === null || py === null) {
      return null;
    }
    points.push({ x: px, y: py });
  }
  return points;
}

export function specFromState(state: DimensionFormState, units: Units): DimensionSpec | null {
  const points = typedPoints(state, units);
  return points === null
    ? null
    : specFromPoints(state.kind, points, lengthValue(state.radius, units) ?? 0, state.showDiameter);
}

/** Places the next click and mirrors it into the typed fields. */
export function placePoint(
  state: DimensionFormState,
  units: Units,
  point: Point,
): DimensionFormState {
  const placed = [...state.placed, point].slice(0, pointCount(state.kind));
  const next = { ...state, placed };
  placed.forEach((p, index) => {
    const pair = PAIRS[index];
    if (pair !== undefined) {
      next[pair[0]] = lengthText(p.x, units);
      next[pair[1]] = lengthText(p.y, units);
    }
  });
  return next;
}

export function clearPlaced(state: DimensionFormState): DimensionFormState {
  return { ...state, placed: [] };
}

/** Whether the click just placed completes the dimension. */
export function isComplete(state: DimensionFormState): boolean {
  return state.placed.length >= pointCount(state.kind);
}

/** Guide lines while placing: the placed points joined, then on to the cursor. */
export function placingPreview(state: DimensionFormState, cursor: Point | null): Path[] {
  const [first, ...rest] = state.placed;
  if (first === undefined || cursor === null) {
    return [];
  }
  const segments = [...rest, cursor].map((to) => ({ kind: 'line' as const, to }));
  return [{ subpaths: [{ start: first, closed: false, segments }] }];
}

/** Picking the arc or circle a radius or diameter dimension measures: its centre is the first point. */
export function pickCircle(
  state: DimensionFormState,
  units: Units,
  centre: Point,
  radius: number,
): DimensionFormState {
  return { ...placePoint(clearPlaced(state), units, centre), radius: lengthText(radius, units) };
}

/** Whether a point lies on a dimension's text, within `tolerance` millimetres. */
export function onDimensionText(
  box: readonly [number, number, number, number] | undefined,
  point: Point,
  tolerance: number,
): boolean {
  if (box === undefined) {
    return false;
  }
  const [x0, y0, x1, y1] = box;
  return (
    point.x >= x0 - tolerance &&
    point.x <= x1 + tolerance &&
    point.y >= y0 - tolerance &&
    point.y <= y1 + tolerance
  );
}

/**
 * Dragging a dimension's text from `grab` to `pointer`: the dimension line
 * follows the pointer across (its offset, an angle's radius), and the text
 * stays in the middle of the line unless `free` (Shift held), when it slides
 * along the line to the pointer. A radius dimension or a note moves its text
 * point instead.
 */
export function dragDimensionText(
  spec: DimensionSpec,
  grab: Point,
  pointer: Point,
  free: boolean,
): { spec: DimensionSpec; textAt: Point | null } {
  const dx = pointer.x - grab.x;
  const dy = pointer.y - grab.y;
  const textAt = free ? pointer : null;
  switch (spec.kind) {
    case 'horizontal':
      return { spec: { ...spec, offset: spec.offset + dy }, textAt };
    case 'vertical':
      return { spec: { ...spec, offset: spec.offset + dx }, textAt };
    case 'linear': {
      const length = Math.hypot(spec.to.x - spec.from.x, spec.to.y - spec.from.y);
      if (length < 1e-9) {
        return { spec, textAt };
      }
      // The left normal of the direction from the first point to the second.
      const nx = -(spec.to.y - spec.from.y) / length;
      const ny = (spec.to.x - spec.from.x) / length;
      return { spec: { ...spec, offset: spec.offset + dx * nx + dy * ny }, textAt };
    }
    case 'angular': {
      const before = Math.hypot(grab.x - spec.centre.x, grab.y - spec.centre.y);
      const after = Math.hypot(pointer.x - spec.centre.x, pointer.y - spec.centre.y);
      const radius = Math.max(1e-3, spec.radius + after - before);
      return { spec: { ...spec, radius }, textAt };
    }
    case 'radius':
      return { spec: { ...spec, at: { x: spec.at.x + dx, y: spec.at.y + dy } }, textAt: null };
    case 'note':
      return {
        spec: { ...spec, textAt: { x: spec.textAt.x + dx, y: spec.textAt.y + dy } },
        textAt: null,
      };
  }
}
