// Pure state for the Extrude and Weave form: the drive rails (their
// direction and start points), what they carry (vector cross sections
// attached to the rails, or a component stretched or repeated along them),
// the weave at the crossings, the kernel payload and the view overlays.

import type { EntitySummary, Path, Point } from '../../bridge/document';
import type {
  Combine,
  ComponentFill,
  ExtrudeParams,
  ExtrudeStation,
  WeaveMode,
} from '../../bridge/model';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
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
} from '../TwoRailSweepForm/rails';

/** What the rails carry. */
export type ExtrudeSource = 'vectors' | 'component';

/** A cross section attached to a rail: its vector and where it sits. */
export interface AttachedSection {
  rail: number;
  section: number;
  at: Point;
  smooth: boolean;
}

export interface ExtrudeFormState {
  name: string;
  combine: Combine;
  /** Drive rail ids in the order chosen. */
  rails: number[];
  reverse: boolean[];
  starts: (Point | null)[];
  source: ExtrudeSource;
  stations: AttachedSection[];
  sharpCorners: boolean;
  betweenSpans: boolean;
  /** The component to extrude, null for the first one listed. */
  component: number | null;
  fill: ComponentFill;
  /** Percent. */
  overlap: string;
  flipAlternate: boolean;
  weave: boolean;
  weaveMode: WeaveMode;
  /** Percent. */
  zUnder: string;
  /** Percent. */
  zOver: string;
  exactHeight: boolean;
  /** Display unit. */
  height: string;
}

export type ExtrudeField = 'overlap' | 'zUnder' | 'zOver' | 'height';
export type ExtrudeErrors = Partial<Record<ExtrudeField, string>>;

export function initialExtrudeState(units: Units): ExtrudeFormState {
  return {
    name: '',
    combine: 'add',
    rails: [],
    reverse: [],
    starts: [],
    source: 'vectors',
    stations: [],
    sharpCorners: false,
    betweenSpans: false,
    component: null,
    fill: 'stretch',
    overlap: '0',
    flipAlternate: false,
    weave: false,
    weaveMode: 'scaleShape',
    zUnder: '70',
    zOver: '130',
    exactHeight: false,
    height: lengthText(units === 'inch' ? 12.7 : 10, units),
  };
}

export function convertExtrudeUnits(
  state: ExtrudeFormState,
  from: Units,
  to: Units,
): ExtrudeFormState {
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

/** Rails from the selection: one or more vectors, open or closed, in the
 * order selected; null when anything else is selected or nothing is. */
export function railsFromSelection(
  ids: readonly number[],
  entities: readonly EntitySummary[],
): number[] | null {
  const vectors = new Set(entities.filter((e) => e.kind === 'path').map((e) => e.id));
  return ids.length > 0 && ids.every((id) => vectors.has(id)) ? [...ids] : null;
}

/** New rails clear everything tied to the old ones. */
export function withRails(state: ExtrudeFormState, rails: number[]): ExtrudeFormState {
  return {
    ...state,
    rails,
    reverse: rails.map(() => false),
    starts: rails.map(() => null),
    stations: [],
  };
}

/** Reverse Rail: rail `k` runs the other way. */
export function reverseRail(state: ExtrudeFormState, k: number): ExtrudeFormState {
  return {
    ...state,
    reverse: state.rails.map((_, i) => (state.reverse[i] ?? false) !== (i === k)),
  };
}

/** Make or Insert Start Point on closed rail `k`. */
export function setStart(state: ExtrudeFormState, k: number, point: Point): ExtrudeFormState {
  return {
    ...state,
    starts: state.rails.map((_, i) => (i === k ? point : (state.starts[i] ?? null))),
  };
}

/** Make Start Point snaps to the nearest node; Insert Start Point uses the
 * clicked place itself. */
export function startPointFor(rail: Rail, click: Point, insert: boolean): Point {
  return insert ? nearestOnRail(rail, click).point : nearestNode(rail, click);
}

function sortOn(stations: AttachedSection[], rails: readonly (Rail | null)[]): AttachedSection[] {
  const f = (s: AttachedSection) => {
    const rail = rails[s.rail];
    return rail === null || rail === undefined ? 0 : nearestOnRail(rail, s.at).f;
  };
  return [...stations].sort((p, q) => p.rail - q.rail || f(p) - f(q));
}

/** Attaches `section` where the user clicked on rail `k`. The first
 * section attached to a rail goes at its start and (open rails) its end;
 * later ones sit at the click. */
export function attachSection(
  state: ExtrudeFormState,
  rails: readonly (Rail | null)[],
  k: number,
  section: number,
  click: Point,
): ExtrudeFormState {
  const rail = rails[k];
  if (rail === null || rail === undefined) {
    return state;
  }
  if (!state.stations.some((s) => s.rail === k)) {
    const ends: AttachedSection[] = [{ rail: k, section, at: pointAt(rail, 0), smooth: true }];
    if (!rail.closed) {
      ends.push({ rail: k, section, at: pointAt(rail, 1), smooth: true });
    }
    return { ...state, stations: sortOn([...state.stations, ...ends], rails) };
  }
  const added = { rail: k, section, at: nearestOnRail(rail, click).point, smooth: true };
  return { ...state, stations: sortOn([...state.stations, added], rails) };
}

/** The attached section whose marker is nearest `p` within `tolerance`. */
export function stationAt(state: ExtrudeFormState, p: Point, tolerance: number): number | null {
  let best: number | null = null;
  let bestD = tolerance;
  for (const [index, s] of state.stations.entries()) {
    const d = Math.hypot(s.at.x - p.x, s.at.y - p.y);
    if (d <= bestD) {
      best = index;
      bestD = d;
    }
  }
  return best;
}

/** Drags an attached section along its rail to the point nearest `p`. */
export function moveStation(
  state: ExtrudeFormState,
  rails: readonly (Rail | null)[],
  index: number,
  p: Point,
): ExtrudeFormState {
  return {
    ...state,
    stations: state.stations.map((s, i) => {
      const rail = rails[s.rail];
      return i === index && rail !== null && rail !== undefined
        ? { ...s, at: nearestOnRail(rail, p).point }
        : s;
    }),
  };
}

export function deleteStation(state: ExtrudeFormState, index: number): ExtrudeFormState {
  return { ...state, stations: state.stations.filter((_, i) => i !== index) };
}

export function toggleSmooth(state: ExtrudeFormState, index: number): ExtrudeFormState {
  return {
    ...state,
    stations: state.stations.map((s, i) => (i === index ? { ...s, smooth: !s.smooth } : s)),
  };
}

/** Remove All Cross Sections from rail `k`. */
export function removeAllOn(state: ExtrudeFormState, k: number): ExtrudeFormState {
  return { ...state, stations: state.stations.filter((s) => s.rail !== k) };
}

/** Add To All Rail Nodes: the section at every node of its rail. */
export function addToAllNodes(
  state: ExtrudeFormState,
  rails: readonly (Rail | null)[],
  index: number,
): ExtrudeFormState {
  const station = state.stations[index];
  const rail = station === undefined ? undefined : rails[station.rail];
  if (station === undefined || rail === null || rail === undefined) {
    return state;
  }
  const others = state.stations.filter((s) => s.rail !== station.rail);
  const nodes = rail.nodes.map((f) => ({
    rail: station.rail,
    section: station.section,
    at: pointAt(rail, f),
    smooth: true,
  }));
  return { ...state, stations: sortOn([...others, ...nodes], rails) };
}

/** A station at the very start or end of an open rail stays at 0 or 1. */
function fractionOf(rail: Rail, p: Point): number {
  const f = nearestOnRail(rail, p).f;
  if (f < 1e-6) {
    return 0;
  }
  return f > 1 - 1e-6 ? 1 : f;
}

export function validateExtrude(state: ExtrudeFormState, units: Units): ExtrudeErrors {
  const errors: ExtrudeErrors = {};
  const checks: Array<[ExtrudeField, string | undefined]> = [];
  if (state.source === 'component' && state.fill === 'copies') {
    checks.push(['overlap', fieldError(numberValue(state.overlap), { min: 0, max: 95 })]);
  }
  if (state.weave) {
    checks.push(['zUnder', fieldError(numberValue(state.zUnder), { min: 0 })]);
    checks.push(['zOver', fieldError(numberValue(state.zOver), { min: 0 })]);
  }
  if (state.exactHeight) {
    checks.push(['height', fieldError(lengthValue(state.height, units), { positive: true })]);
  }
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** The cross sections to extrude: the attached ones, or with none attached
 * the one open vector selected apart from the rails. */
export function sectionsToUse(state: ExtrudeFormState, selectedOpen: readonly number[]): number[] {
  if (state.stations.length > 0) {
    return [...new Set(state.stations.map((s) => s.section))];
  }
  const loose = selectedOpen.filter((id) => !state.rails.includes(id));
  return loose.length === 1 ? loose : [];
}

/** The component the rails carry: the chosen one if it still exists, or
 * the first listed. */
export function componentToUse(
  state: ExtrudeFormState,
  components: readonly { id: number }[],
): number | null {
  if (state.component !== null && components.some((c) => c.id === state.component)) {
    return state.component;
  }
  return components[0]?.id ?? null;
}

export interface ExtrudeArgs {
  rails: number[];
  sections: number[];
  component?: number;
  params: ExtrudeParams;
}

/** Kernel arguments, or null while something is missing or wrong. */
export function extrudeArgs(
  state: ExtrudeFormState,
  rails: readonly (Rail | null)[],
  selectedOpen: readonly number[],
  components: readonly { id: number }[],
  units: Units,
): ExtrudeArgs | null {
  if (state.rails.length === 0 || Object.keys(validateExtrude(state, units)).length > 0) {
    return null;
  }
  const params: ExtrudeParams = {
    reverse: state.rails.map((_, i) => state.reverse[i] ?? false),
    starts: state.rails.map((_, i) => state.starts[i] ?? null),
    stations: [],
    sharpCorners: state.sharpCorners,
    sweepBetweenSpans: state.betweenSpans,
    fill: state.fill,
    overlap: numberValue(state.overlap) ?? 0,
    flipAlternate: state.flipAlternate,
    weave: state.weave,
    weaveMode: state.weaveMode,
    zUnder: numberValue(state.zUnder) ?? 70,
    zOver: numberValue(state.zOver) ?? 130,
    exactHeight: state.exactHeight,
    height: lengthValue(state.height, units) ?? 10,
  };
  if (state.source === 'component') {
    const component = componentToUse(state, components);
    return component === null ? null : { rails: [...state.rails], sections: [], component, params };
  }
  const sections = sectionsToUse(state, selectedOpen);
  if (sections.length === 0) {
    return null;
  }
  const stations: ExtrudeStation[] = [];
  for (const s of state.stations) {
    const rail = rails[s.rail];
    if (rail !== null && rail !== undefined) {
      stations.push({
        rail: s.rail,
        section: sections.indexOf(s.section),
        at: fractionOf(rail, s.at),
        smooth: s.smooth,
      });
    }
  }
  return { rails: [...state.rails], sections, params: { ...params, stations } };
}

/** Sweep between spans applies when every section has as many spans. */
export function spansMatch(paths: readonly (Path | undefined)[]): boolean {
  const counts = paths.map((p) => (p === undefined ? -1 : (p.subpaths[0]?.segments.length ?? -1)));
  const first = counts[0];
  return first !== undefined && first > 0 && counts.every((c) => c === first);
}

/** What the drawing view shows: each rail with a start marker and
 * direction arrows, and a node where each section is attached. */
export function extrudeOverlays(
  state: ExtrudeFormState,
  rails: readonly (Rail | null)[],
  railPaths: readonly (Path | undefined)[],
  mmPerPx: number,
): PreviewOverlay[] {
  const out: PreviewOverlay[] = [];
  rails.forEach((rail, k) => {
    const path = railPaths[k];
    if (path !== undefined) {
      out.push({ path, tone: 'first' });
    }
    if (rail === null) {
      return;
    }
    out.push({ path: markerPath(pointAt(rail, 0), 8 * mmPerPx), tone: 'second', fill: true });
    const arrows = Math.max(2, Math.min(12, Math.round(railLength(rail) / (60 * mmPerPx))));
    for (const arrow of arrowPaths(rail, arrows, 5 * mmPerPx)) {
      out.push({ path: arrow, tone: 'first' });
    }
  });
  for (const s of state.stations) {
    const rail = rails[s.rail];
    if (rail === null || rail === undefined) {
      continue;
    }
    // A short tick across the rail where the section hangs.
    const f = nearestOnRail(rail, s.at).f;
    const ahead = pointAt(rail, Math.min(1, f + 1e-3));
    const behind = pointAt(rail, Math.max(0, f - 1e-3));
    const dx = ahead.x - behind.x;
    const dy = ahead.y - behind.y;
    const len = Math.hypot(dx, dy) || 1;
    const half = 10 * mmPerPx;
    const nx = (-dy / len) * half;
    const ny = (dx / len) * half;
    out.push({
      path: polylinePath(
        [
          { x: s.at.x + nx, y: s.at.y + ny },
          { x: s.at.x - nx, y: s.at.y - ny },
        ],
        false,
      ),
      tone: 'accent',
    });
    out.push({ path: markerPath(s.at, 6 * mmPerPx), tone: 'accent', fill: !s.smooth });
  }
  return out;
}
