// Pure state for the Measure tool: the two picked points, the live readout
// while the second one follows the pointer, and the rows shown for the
// vector under the first click.

import type { Path, Point } from '../../bridge/document';
import type { MeasureHit } from '../../bridge/drawTools';
import type { Units } from '../../bridge/types';
import { measureBetween } from '../../lib/measure';
import { strings } from '../../lib/formStrings';
import { formatDisplay, formatLength, unitSuffix } from '../../lib/units';

const text = strings.drawTools.measure;

export interface MeasureState {
  from: Point | null;
  to: Point | null;
  /** True once the second point is placed; the next press starts again. */
  fixed: boolean;
}

export const EMPTY_MEASURE: MeasureState = { from: null, to: null, fixed: false };

/** A press: starts a new measurement, or places the second point. */
export function measurePress(state: MeasureState, p: Point): MeasureState {
  if (state.from === null || state.fixed) {
    return { from: p, to: p, fixed: false };
  }
  return { from: state.from, to: p, fixed: true };
}

/** The pointer moved: the loose end follows it. */
export function measureMove(state: MeasureState, p: Point): MeasureState {
  if (state.from === null || state.fixed) {
    return state;
  }
  return { ...state, to: p };
}

/** A release: after a drag the second point lands where the pointer let go;
 * after a click the tool waits for a second click. */
export function measureRelease(state: MeasureState, p: Point, dragged: boolean): MeasureState {
  if (state.from === null || state.fixed || !dragged) {
    return state;
  }
  return { from: state.from, to: p, fixed: true };
}

/** True when a press starts a new measurement, which also picks a vector. */
export function startsNewMeasurement(state: MeasureState): boolean {
  return state.from === null || state.fixed;
}

export interface ReadoutRow {
  label: string;
  value: string;
}

function length(mm: number, units: Units): string {
  return `${formatLength(mm, units)} ${unitSuffix(units)}`;
}

function angle(deg: number): string {
  return `${formatDisplay(deg, 'mm')} ${strings.units.degrees}`;
}

/** Measure Between 2 Points, Span or Contour Properties. */
export type MeasureMode = 'points' | 'span' | 'contour';

function point(p: Point, units: Units): string {
  return `${formatLength(p.x, units)}, ${formatLength(p.y, units)} ${unitSuffix(units)}`;
}

/** First Point and Cursor Position, then distance, angle and the X and Y
 * distances between the two points once the first is placed. */
export function pointRows(state: MeasureState, cursor: Point | null, units: Units): ReadoutRow[] {
  const rows: ReadoutRow[] = [];
  if (state.from !== null) {
    rows.push({ label: text.firstPoint, value: point(state.from, units) });
  }
  const at = state.fixed ? state.to : cursor;
  if (at !== null) {
    rows.push({ label: text.cursor, value: point(at, units) });
  }
  if (state.from === null || state.to === null) {
    return rows;
  }
  const m = measureBetween(state.from, state.to);
  return [
    ...rows,
    { label: text.distance, value: length(m.distance, units) },
    { label: text.angle, value: angle(m.angleDeg) },
    { label: text.dx, value: length(m.dx, units) },
    { label: text.dy, value: length(m.dy, units) },
  ];
}

/** Span: the span under the click, its type, length and ends (and the
 * radius of an arc). */
export function spanRows(hit: MeasureHit, units: Units): ReadoutRow[] {
  const rows: ReadoutRow[] = [
    { label: text.type, value: text.kinds[hit.segment.kind] },
    { label: text.length, value: length(hit.segment.length, units) },
    { label: text.startPoint, value: point(hit.segment.start, units) },
    { label: text.endPoint, value: point(hit.segment.end, units) },
  ];
  const { radius, sweepDeg } = hit.segment;
  if (radius !== undefined) {
    rows.push({ label: text.radius, value: length(radius, units) });
  }
  if (sweepDeg !== undefined) {
    rows.push({ label: text.sweep, value: angle(Math.abs(sweepDeg)) });
  }
  return rows;
}

/** Contour Properties: area, perimeter and number of spans. */
export function contourRows(hit: MeasureHit, units: Units): ReadoutRow[] {
  return [
    { label: text.area, value: hit.closed ? areaText(hit.area, units) : text.openArea },
    { label: text.perimeter, value: length(hit.length, units) },
    { label: text.spans, value: String(hit.spans) },
  ];
}

/** An area in the square display unit. */
export function areaText(mm2: number, units: Units): string {
  const scale = units === 'inch' ? 25.4 * 25.4 : 1;
  return `${formatDisplay(mm2 / scale, units)} ${text.square(unitSuffix(units))}`;
}

/** The span a Span pick highlights: the picked segment on its own. */
export function spanPath(hit: MeasureHit): Path {
  const { start, end, kind } = hit.segment;
  if (kind === 'line') {
    return { subpaths: [{ start, closed: false, segments: [{ kind: 'line', to: end }] }] };
  }
  // Arcs and curves: the vector's own segment, found by its ends.
  for (const sub of hit.path.subpaths) {
    let from = sub.start;
    for (const seg of sub.segments) {
      if (
        Math.hypot(from.x - start.x, from.y - start.y) < 1e-9 &&
        Math.hypot(seg.to.x - end.x, seg.to.y - end.y) < 1e-9
      ) {
        return { subpaths: [{ start: from, closed: false, segments: [seg] }] };
      }
      from = seg.to;
    }
  }
  return { subpaths: [{ start, closed: false, segments: [{ kind: 'line', to: end }] }] };
}

/** The ruler line drawn in the view. */
export function rulerPreview(state: MeasureState): Path[] {
  if (state.from === null || state.to === null) {
    return [];
  }
  if (state.from.x === state.to.x && state.from.y === state.to.y) {
    return [];
  }
  return [
    {
      subpaths: [{ start: state.from, closed: false, segments: [{ kind: 'line', to: state.to }] }],
    },
  ];
}
