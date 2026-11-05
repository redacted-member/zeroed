// Pure state for Text On A Curve: which text and vector it works on, and
// the settings kept on the text (size, spacing, position, alignment and
// orientation), plus the arc length maths of sliding the text along.

import type { CurvePosition, EntitySummary, OnCurve, Path, Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { flattenPath } from '../../lib/geometry';

export interface OnCurveState {
  fill: boolean;
  /** Text Spacing, percent. */
  spacing: number;
  position: CurvePosition;
  otherSide: boolean;
  offset: string;
  align: OnCurve['align'];
  keepVertical: boolean;
  /** Millimetres the text is slid along the curve. */
  startAt: number;
}

export const SPACING_MIN = 10;
export const SPACING_MAX = 300;

export function initialOnCurveState(units: Units): OnCurveState {
  return {
    fill: false,
    spacing: 100,
    position: 'above',
    otherSide: false,
    offset: lengthText(0, units),
    align: 'centre',
    keepVertical: false,
    startAt: 0,
  };
}

/** The form for text already on a curve. Text Spacing starts at 100% again,
 *'s slider does, so it scales what the text has. */
export function stateFromCurve(curve: OnCurve, units: Units): OnCurveState {
  return {
    fill: curve.fill,
    spacing: 100,
    position: curve.position,
    otherSide: curve.otherSide,
    offset: lengthText(curve.offset, units),
    align: curve.align,
    keepVertical: curve.keepVertical,
    startAt: curve.startAt,
  };
}

export function convertOnCurveUnits(state: OnCurveState, from: Units, to: Units): OnCurveState {
  return convertLengthFields(state, ['offset'], from, to);
}

export function offsetError(state: OnCurveState, units: Units): string | undefined {
  return fieldError(lengthValue(state.offset, units), { min: 0 });
}

/** The settings for the kernel, or null while the offset is not valid.
 * `baseSpacing` is the spacing the text had when the form opened. */
export function curveOf(
  state: OnCurveState,
  units: Units,
  pathId: number,
  baseSpacing = 100,
): OnCurve | null {
  const offset = lengthValue(state.offset, units);
  if (offset === null || offset < 0) {
    return null;
  }
  return {
    pathId,
    position: state.position,
    otherSide: state.otherSide,
    offset,
    align: state.align,
    keepVertical: state.keepVertical,
    fill: state.fill,
    spacing: (baseSpacing * state.spacing) / 100,
    startAt: state.startAt,
  };
}

/** The text and the vector the form works on: a text and one vector
 * selected, or one text already on a curve. */
export function pickTarget(
  ids: readonly number[],
  entities: readonly EntitySummary[],
): { textId: number; pathId: number } | null {
  const picked = ids
    .map((id) => entities.find((e) => e.id === id))
    .filter((e): e is EntitySummary => e !== undefined);
  const texts = picked.filter((e) => e.kind === 'text');
  const paths = picked.filter((e) => e.kind === 'path');
  const [text] = texts;
  if (text === undefined || texts.length !== 1) {
    return null;
  }
  const [path] = paths;
  if (path !== undefined && paths.length === 1 && picked.length === 2) {
    return { textId: text.id, pathId: path.id };
  }
  if (picked.length === 1 && text.onCurve !== undefined) {
    return { textId: text.id, pathId: text.onCurve };
  }
  return null;
}

/** Arc length along `path` of the point on it nearest `p`, in millimetres. */
export function arcLengthAt(path: Path, p: Point): number {
  let walked = 0;
  let best = { d: Infinity, s: 0 };
  for (const sub of flattenPath(path, 0.05)) {
    const n = sub.points.length / 2;
    const count = sub.closed ? n : n - 1;
    for (let i = 0; i < count; i += 1) {
      const j = (i + 1) % n;
      const ax = sub.points[2 * i] ?? 0;
      const ay = sub.points[2 * i + 1] ?? 0;
      const bx = sub.points[2 * j] ?? 0;
      const by = sub.points[2 * j + 1] ?? 0;
      const dx = bx - ax;
      const dy = by - ay;
      const length = Math.hypot(dx, dy);
      const t =
        length < 1e-12
          ? 0
          : Math.min(1, Math.max(0, ((p.x - ax) * dx + (p.y - ay) * dy) / (length * length)));
      const d = Math.hypot(ax + dx * t - p.x, ay + dy * t - p.y);
      if (d < best.d) {
        best = { d, s: walked + length * t };
      }
      walked += length;
    }
  }
  return best.s;
}

/** The slide a drag from `from` to `to` gives, starting from `startAt`;
 * with the text on the other side it runs the other way along the curve. */
export function slideTo(
  path: Path,
  from: Point,
  to: Point,
  startAt: number,
  otherSide: boolean,
): number {
  const moved = arcLengthAt(path, to) - arcLengthAt(path, from);
  return startAt + (otherSide ? -moved : moved);
}
