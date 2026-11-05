// Pure state for the Turn and Spin form.

import type { EntitySummary, Path, Point } from '../../bridge/document';
import type { Combine, SpinParams, TurnParams } from '../../bridge/model';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { flattenPath } from '../../lib/geometry';

export type RevolveMode = 'turn' | 'spin';

export interface TurnSpinState {
  name: string;
  mode: RevolveMode;
  exactHeight: boolean;
  /** Display unit. */
  height: string;
  /** Display unit. */
  combine: Combine;
}

export type TurnSpinField = 'height';
export type TurnSpinErrors = Partial<Record<TurnSpinField, string>>;

export type RevolveArgs =
  { mode: 'turn'; params: TurnParams } | { mode: 'spin'; params: SpinParams };

export function initialTurnSpinState(units: Units): TurnSpinState {
  return {
    name: '',
    mode: 'turn',
    exactHeight: false,
    height: lengthText(units === 'inch' ? 12.7 : 10, units),
    combine: 'add',
  };
}

export function convertTurnSpinUnits(state: TurnSpinState, from: Units, to: Units): TurnSpinState {
  return convertLengthFields(state, ['height'], from, to);
}

/** The profile vector: the one selected object when it is an open vector, else null. */
export function profileFromSelection(
  ids: readonly number[],
  entities: readonly EntitySummary[],
): number | null {
  const [id] = ids;
  if (ids.length !== 1 || id === undefined) {
    return null;
  }
  const entity = entities.find((e) => e.id === id);
  return entity !== undefined && entity.kind === 'path' && !entity.closed ? id : null;
}

export function validateTurnSpin(state: TurnSpinState, units: Units): TurnSpinErrors {
  const errors: TurnSpinErrors = {};
  if (state.exactHeight) {
    const height = fieldError(lengthValue(state.height, units), { positive: true });
    if (height !== undefined) {
      errors.height = height;
    }
  }
  return errors;
}

/** Kernel arguments for the chosen mode, or null while the form is not valid. */
export function revolveArgs(state: TurnSpinState, units: Units): RevolveArgs | null {
  if (Object.keys(validateTurnSpin(state, units)).length > 0) {
    return null;
  }
  const params = {
    // The form has no base height: Component Properties sets it.
    baseHeight: 0,
    exactHeight: state.exactHeight,
    height: lengthValue(state.height, units) ?? 10,
  };
  return state.mode === 'turn' ? { mode: 'turn', params } : { mode: 'spin', params };
}

/** Where a profile path ends: the last segment's end, or its start. */
export function profileEnds(path: Path): { start: Point; end: Point } | null {
  const sub = path.subpaths[0];
  if (sub === undefined) {
    return null;
  }
  return { start: sub.start, end: sub.segments[sub.segments.length - 1]?.to ?? sub.start };
}

/** The end Spin turns round: the one further left (lower X). */
export function spinCentre(path: Path): Point | null {
  const ends = profileEnds(path);
  if (ends === null) {
    return null;
  }
  const { start, end } = ends;
  return end.x < start.x || (end.x === start.x && end.y < start.y) ? end : start;
}

/** Spin radius: the furthest the profile reaches across in X from its left end. */
export function spinRadius(path: Path): number {
  const centre = spinCentre(path);
  if (centre === null) {
    return 0;
  }
  let radius = 0;
  for (const sub of flattenPath(path, 0.05)) {
    for (let i = 0; i < sub.points.length; i += 2) {
      radius = Math.max(radius, Math.abs((sub.points[i] ?? centre.x) - centre.x));
    }
  }
  return radius;
}

/** A full circle round `centre`, in two arcs. */
function circle(centre: Point, radius: number): Path {
  return {
    subpaths: [
      {
        start: { x: centre.x + radius, y: centre.y },
        closed: true,
        segments: [
          { kind: 'arc', to: { x: centre.x - radius, y: centre.y }, bulge: 1 },
          { kind: 'arc', to: { x: centre.x + radius, y: centre.y }, bulge: 1 },
        ],
      },
    ],
  };
}

/** Preview paths: the profile, and for Turn the axis from its start to its
 * end, for Spin the circle its reach sweeps round the left end. */
export function revolvePreview(state: TurnSpinState, profile: Path | undefined): Path[] {
  const ends = profile === undefined ? null : profileEnds(profile);
  if (profile === undefined || ends === null) {
    return [];
  }
  if (state.mode === 'turn') {
    const axis: Path = {
      subpaths: [{ start: ends.start, segments: [{ kind: 'line', to: ends.end }], closed: false }],
    };
    return [profile, axis];
  }
  const centre = spinCentre(profile);
  const radius = spinRadius(profile);
  return centre === null || radius <= 0 ? [profile] : [profile, circle(centre, radius)];
}
