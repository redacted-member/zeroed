// Pure state for the Two-Rail Sweep form: the drive rails (their order,
// direction and start points), the cross sections attached to them and
// where, the sweep options, the kernel payload and the view overlays.

import type { EntitySummary, Path, Point } from '../../bridge/document';
import type { Combine, SweepParams, SweepStation } from '../../bridge/model';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { polylinePath } from '../../lib/geometry';
import type { PreviewOverlay } from '../../state/drawStore';
import {
  arrowPaths,
  markerPath,
  nearestNode,
  nearestOnRail,
  pointAt,
  railLength,
  type Rail,
} from './rails';

/** A cross section attached to the rails: its vector and where each end
 * sits (job points on the first and second rail). */
export interface AttachedSection {
  section: number;
  a: Point;
  b: Point;
  smooth: boolean;
}

export interface SweepFormState {
  name: string;
  combine: Combine;
  /** First (red) and second (green) rail ids, or empty. */
  rails: number[];
  reverse: [boolean, boolean];
  /** Chosen start of each closed rail. */
  starts: [Point | null, Point | null];
  stations: AttachedSection[];
  scaleWithWidth: boolean;
  betweenSpans: boolean;
  fillInner: boolean;
  exactHeight: boolean;
  /** Display unit. */
  height: string;
}

export type SweepField = 'height';
export type SweepErrors = Partial<Record<SweepField, string>>;

export interface SweepArgs {
  rails: [number, number];
  sections: number[];
  params: SweepParams;
}

export function initialSweepState(units: Units): SweepFormState {
  return {
    name: '',
    combine: 'add',
    rails: [],
    reverse: [false, false],
    starts: [null, null],
    stations: [],
    scaleWithWidth: true,
    betweenSpans: false,
    fillInner: false,
    exactHeight: false,
    height: lengthText(units === 'inch' ? 12.7 : 10, units),
  };
}

export function convertSweepUnits(state: SweepFormState, from: Units, to: Units): SweepFormState {
  return convertLengthFields(state, ['height'], from, to);
}

/** The selected vectors that are open, in selection order. */
export function openVectorIds(
  ids: readonly number[],
  entities: readonly EntitySummary[],
): number[] {
  const open = new Set(entities.filter((e) => e.kind === 'path' && !e.closed).map((e) => e.id));
  return ids.filter((id) => open.has(id));
}

/** Rails from the selection: exactly two vectors, open or closed, the
 * first selected first; null otherwise. */
export function railsFromSelection(
  ids: readonly number[],
  entities: readonly EntitySummary[],
): number[] | null {
  const vectors = new Set(entities.filter((e) => e.kind === 'path').map((e) => e.id));
  return ids.length === 2 && ids.every((id) => vectors.has(id)) ? [...ids] : null;
}

/** New rails clear everything tied to the old ones. */
export function withRails(state: SweepFormState, rails: number[]): SweepFormState {
  return { ...state, rails, reverse: [false, false], starts: [null, null], stations: [] };
}

/** Swap Rail Order: the red rail becomes green and green red. */
export function swapRails(state: SweepFormState): SweepFormState {
  const [a, b] = state.rails;
  if (a === undefined || b === undefined) {
    return state;
  }
  return {
    ...state,
    rails: [b, a],
    reverse: [state.reverse[1], state.reverse[0]],
    starts: [state.starts[1], state.starts[0]],
    stations: state.stations.map((s) => ({ ...s, a: s.b, b: s.a })),
  };
}

/** Reverse Rail: rail `k` runs the other way. */
export function reverseRail(state: SweepFormState, k: 0 | 1): SweepFormState {
  const reverse: [boolean, boolean] = [...state.reverse];
  reverse[k] = !reverse[k];
  return { ...state, reverse };
}

/** Make or Insert Start Point on closed rail `k`. */
export function setStart(state: SweepFormState, k: 0 | 1, point: Point): SweepFormState {
  const starts: [Point | null, Point | null] = [...state.starts];
  starts[k] = point;
  return { ...state, starts };
}

/** Attaches `section` where the user clicked on rail `k`. The first
 * section attached goes across the starts and across the ends of the
 * rails; later ones sit at the click, at the same fraction of the other
 * rail. */
export function attachSection(
  state: SweepFormState,
  rails: [Rail, Rail],
  k: 0 | 1,
  section: number,
  click: Point,
): SweepFormState {
  const [ra, rb] = rails;
  if (state.stations.length === 0) {
    const ends: AttachedSection[] = [
      { section, a: pointAt(ra, 0), b: pointAt(rb, 0), smooth: true },
    ];
    if (!ra.closed) {
      ends.push({ section, a: pointAt(ra, 1), b: pointAt(rb, 1), smooth: true });
    }
    return { ...state, stations: ends };
  }
  const f = nearestOnRail(rails[k], click).f;
  const added = { section, a: pointAt(ra, f), b: pointAt(rb, f), smooth: true };
  return { ...state, stations: sortStations([...state.stations, added], ra) };
}

function sortStations(stations: AttachedSection[], ra: Rail): AttachedSection[] {
  return [...stations].sort((p, q) => nearestOnRail(ra, p.a).f - nearestOnRail(ra, q.a).f);
}

/** The station end nearest `p` within `tolerance` mm, if any. */
export function stationEndAt(
  state: SweepFormState,
  p: Point,
  tolerance: number,
): { index: number; end: 'a' | 'b' } | null {
  let best: { index: number; end: 'a' | 'b' } | null = null;
  let bestD = tolerance;
  for (const [index, s] of state.stations.entries()) {
    for (const end of ['a', 'b'] as const) {
      const q = s[end];
      const d = Math.hypot(q.x - p.x, q.y - p.y);
      if (d <= bestD) {
        best = { index, end };
        bestD = d;
      }
    }
  }
  return best;
}

/** The station whose line across the rails passes nearest `p`. */
export function stationAt(state: SweepFormState, p: Point, tolerance: number): number | null {
  let best: number | null = null;
  let bestD = tolerance;
  for (const [index, s] of state.stations.entries()) {
    const dx = s.b.x - s.a.x;
    const dy = s.b.y - s.a.y;
    const len2 = dx * dx + dy * dy;
    const t =
      len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - s.a.x) * dx + (p.y - s.a.y) * dy) / len2));
    const d = Math.hypot(s.a.x + dx * t - p.x, s.a.y + dy * t - p.y);
    if (d <= bestD) {
      best = index;
      bestD = d;
    }
  }
  return best;
}

/** Drags one end of a station along its rail to the point nearest `p`. */
export function moveStationEnd(
  state: SweepFormState,
  rails: [Rail, Rail],
  index: number,
  end: 'a' | 'b',
  p: Point,
): SweepFormState {
  const rail = end === 'a' ? rails[0] : rails[1];
  const point = nearestOnRail(rail, p).point;
  return {
    ...state,
    stations: state.stations.map((s, i) => (i === index ? { ...s, [end]: point } : s)),
  };
}

export function deleteStation(state: SweepFormState, index: number): SweepFormState {
  return { ...state, stations: state.stations.filter((_, i) => i !== index) };
}

export function toggleSmooth(state: SweepFormState, index: number): SweepFormState {
  return {
    ...state,
    stations: state.stations.map((s, i) => (i === index ? { ...s, smooth: !s.smooth } : s)),
  };
}

/** Add To All Rail Nodes: the section at every pair of nodes, when both
 * rails have as many; null when they do not. */
export function addToAllNodes(
  state: SweepFormState,
  rails: [Rail, Rail],
  section: number,
): SweepFormState | null {
  const [ra, rb] = rails;
  if (ra.nodes.length !== rb.nodes.length || ra.nodes.length === 0) {
    return null;
  }
  const stations = ra.nodes.map((fa, i) => ({
    section,
    a: pointAt(ra, fa),
    b: pointAt(rb, rb.nodes[i] ?? fa),
    smooth: true,
  }));
  return { ...state, stations };
}

/** A station end sits exactly on the start/end of an open rail: keep it
 * at 0 or 1 rather than a hair inside. */
function fractionOf(rail: Rail, p: Point): number {
  const f = nearestOnRail(rail, p).f;
  if (f < 1e-6) {
    return 0;
  }
  return f > 1 - 1e-6 ? 1 : f;
}

export function validateSweep(state: SweepFormState, units: Units): SweepErrors {
  const errors: SweepErrors = {};
  if (state.exactHeight) {
    const height = fieldError(lengthValue(state.height, units), { positive: true });
    if (height !== undefined) {
      errors.height = height;
    }
  }
  return errors;
}

/** The cross sections to sweep: the attached ones, or with none attached
 * the one open vector selected apart from the rails. */
export function sectionsToUse(state: SweepFormState, selectedOpen: readonly number[]): number[] {
  if (state.stations.length > 0) {
    return [...new Set(state.stations.map((s) => s.section))];
  }
  const loose = selectedOpen.filter((id) => !state.rails.includes(id));
  return loose.length === 1 ? loose : [];
}

/** Kernel arguments, or null while something is missing or wrong. */
export function sweepArgs(
  state: SweepFormState,
  rails: [Rail, Rail] | null,
  selectedOpen: readonly number[],
  units: Units,
): SweepArgs | null {
  const [a, b] = state.rails;
  const sections = sectionsToUse(state, selectedOpen);
  if (
    a === undefined ||
    b === undefined ||
    rails === null ||
    sections.length === 0 ||
    Object.keys(validateSweep(state, units)).length > 0
  ) {
    return null;
  }
  const stations: SweepStation[] = state.stations.map((s) => ({
    section: sections.indexOf(s.section),
    a: fractionOf(rails[0], s.a),
    b: fractionOf(rails[1], s.b),
    smooth: s.smooth,
  }));
  return {
    rails: [a, b],
    sections,
    params: {
      scaleHeightWithWidth: state.scaleWithWidth,
      // The form has no base height: Component Properties sets it.
      baseHeight: 0,
      reverse: [...state.reverse],
      starts: [...state.starts],
      stations,
      sweepBetweenSpans: state.betweenSpans,
      fillInner: state.fillInner,
      exactHeight: state.exactHeight,
      height: lengthValue(state.height, units) ?? 10,
    },
  };
}

/** Sweep between spans applies when every section has as many spans. */
export function spansMatch(paths: readonly (Path | undefined)[]): boolean {
  const counts = paths.map((p) => (p === undefined ? -1 : (p.subpaths[0]?.segments.length ?? -1)));
  const first = counts[0];
  return first !== undefined && first > 0 && counts.every((c) => c === first);
}

/** Lines across the rails where the sections will flow, between the
 * attached sections as the kernel places them. */
function flowLines(
  stations: readonly AttachedSection[],
  rails: [Rail, Rail],
  count: number,
): Path[] {
  const [ra, rb] = rails;
  const knots = stations
    .map((s) => ({ a: fractionOf(ra, s.a), b: fractionOf(rb, s.b) }))
    .sort((p, q) => p.a - q.a);
  const bAt = (fa: number): number => {
    if (knots.length === 0) {
      return fa;
    }
    const first = knots[0];
    const last = knots[knots.length - 1];
    if (first === undefined || last === undefined) {
      return fa;
    }
    if (fa <= first.a) {
      return first.a > 0 ? (first.b * fa) / first.a : first.b;
    }
    if (fa >= last.a) {
      return last.a < 1 ? last.b + ((1 - last.b) * (fa - last.a)) / (1 - last.a) : last.b;
    }
    for (let i = 0; i + 1 < knots.length; i += 1) {
      const p = knots[i];
      const q = knots[i + 1];
      if (p !== undefined && q !== undefined && fa >= p.a && fa <= q.a) {
        return q.a > p.a ? p.b + ((q.b - p.b) * (fa - p.a)) / (q.a - p.a) : p.b;
      }
    }
    return fa;
  };
  const out: Path[] = [];
  for (let k = 1; k < count; k += 1) {
    const fa = k / count;
    out.push(polylinePath([pointAt(ra, fa), pointAt(rb, bAt(fa))], false));
  }
  return out;
}

/** What the drawing view shows: the rails in red and green with a start
 * marker and direction arrows, the attached sections across them with a
 * node at their first-rail end, and dashed flow lines between. */
export function sweepOverlays(
  state: SweepFormState,
  rails: [Rail, Rail] | null,
  railPaths: readonly (Path | undefined)[],
  mmPerPx: number,
): PreviewOverlay[] {
  if (rails === null) {
    return [];
  }
  const out: PreviewOverlay[] = [];
  const tones = ['first', 'second'] as const;
  rails.forEach((rail, k) => {
    const path = railPaths[k];
    const tone = tones[k] ?? 'first';
    if (path !== undefined) {
      out.push({ path, tone });
    }
    out.push({ path: markerPath(pointAt(rail, 0), 8 * mmPerPx), tone: 'second', fill: true });
    const arrows = Math.max(2, Math.min(12, Math.round(railLength(rail) / (60 * mmPerPx))));
    for (const arrow of arrowPaths(rail, arrows, 5 * mmPerPx)) {
      out.push({ path: arrow, tone });
    }
  });
  if (state.stations.length > 0) {
    for (const line of flowLines(state.stations, rails, 12)) {
      out.push({ path: line, tone: 'accent', dashed: true });
    }
  }
  for (const s of state.stations) {
    out.push({ path: polylinePath([s.a, s.b], false), tone: 'accent' });
    out.push({ path: markerPath(s.a, 6 * mmPerPx), tone: 'accent', fill: true });
  }
  return out;
}

/** Make Start Point snaps to the nearest node; Insert Start Point uses the
 * clicked place itself. */
export function startPointFor(rail: Rail, click: Point, insert: boolean): Point {
  return insert ? nearestOnRail(rail, click).point : nearestNode(rail, click);
}
