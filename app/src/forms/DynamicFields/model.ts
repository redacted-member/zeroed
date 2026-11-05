// Pure state for the Fade and Tilt settings of a component: each is on or
// off, has two anchor points picked in the 3D view, and an amount (fade
// percent) or an angle (tilt degrees).

import type { FadeProp, JobPoint, TiltProp } from '../../bridge/model';
import { fieldError, numberValue } from '../../lib/formFields';

export interface DynamicState {
  fadeOn: boolean;
  /** Keeps its height. */
  fadeFrom: JobPoint | null;
  /** Fades down towards it. */
  fadeTo: JobPoint | null;
  /** Percent of the height taken away at the second anchor, 0 to 100. */
  fadePercent: string;
  tiltOn: boolean;
  /** Stays where it is. */
  tiltPivot: JobPoint | null;
  /** Rises towards it. */
  tiltToward: JobPoint | null;
  /** Degrees, above -90 and below 90. */
  tiltAngle: string;
}

export type DynamicField = 'fadePercent' | 'tiltAngle';
export type DynamicErrors = Partial<Record<DynamicField, string>>;

const DEFAULT_FADE = 50;
const DEFAULT_TILT = 5;

function round(value: number, places: number): string {
  const f = 10 ** places;
  return String(Math.round(value * f) / f);
}

/** The settings a component has now; off when it has none. */
export function dynamicState(fade?: FadeProp | null, tilt?: TiltProp | null): DynamicState {
  return {
    fadeOn: fade !== null && fade !== undefined,
    fadeFrom: fade?.from ?? null,
    fadeTo: fade?.to ?? null,
    fadePercent: round((fade?.amount ?? DEFAULT_FADE / 100) * 100, 1),
    tiltOn: tilt !== null && tilt !== undefined,
    tiltPivot: tilt?.pivot ?? null,
    tiltToward: tilt?.toward ?? null,
    tiltAngle: round(tilt?.angleDeg ?? DEFAULT_TILT, 2),
  };
}

export function validateDynamic(state: DynamicState): DynamicErrors {
  const errors: DynamicErrors = {};
  if (state.fadeOn) {
    const error = fieldError(numberValue(state.fadePercent), { min: 0, max: 100 });
    if (error !== undefined) {
      errors.fadePercent = error;
    }
  }
  if (state.tiltOn) {
    const angle = numberValue(state.tiltAngle);
    const error = fieldError(angle, { min: -89.9, max: 89.9 });
    if (error !== undefined) {
      errors.tiltAngle = error;
    }
  }
  return errors;
}

function apart(a: JobPoint | null, b: JobPoint | null): [JobPoint, JobPoint] | null {
  if (a === null || b === null || Math.hypot(b.x - a.x, b.y - a.y) < 1e-6) {
    return null;
  }
  return [a, b];
}

/** The fade to send: null when off, undefined while on without both
 * anchors or with a bad amount (nothing to apply yet). */
export function fadeProp(state: DynamicState): FadeProp | null | undefined {
  if (!state.fadeOn) {
    return null;
  }
  const points = apart(state.fadeFrom, state.fadeTo);
  const percent = numberValue(state.fadePercent);
  if (points === null || validateDynamic(state).fadePercent !== undefined || percent === null) {
    return undefined;
  }
  return { from: points[0], to: points[1], amount: percent / 100 };
}

/** The tilt to send, as [`fadeProp`] does for the fade. */
export function tiltProp(state: DynamicState): TiltProp | null | undefined {
  if (!state.tiltOn) {
    return null;
  }
  const points = apart(state.tiltPivot, state.tiltToward);
  const angle = numberValue(state.tiltAngle);
  if (points === null || validateDynamic(state).tiltAngle !== undefined || angle === null) {
    return undefined;
  }
  return { pivot: points[0], toward: points[1], angleDeg: angle };
}

/** Puts the two picked points in as the fade's or the tilt's anchors. */
export function withAnchors(
  state: DynamicState,
  purpose: 'fade' | 'tilt',
  points: readonly JobPoint[],
): DynamicState {
  const [first, second] = points;
  if (first === undefined || second === undefined) {
    return state;
  }
  return purpose === 'fade'
    ? { ...state, fadeOn: true, fadeFrom: first, fadeTo: second }
    : { ...state, tiltOn: true, tiltPivot: first, tiltToward: second };
}

function samePoint(a: JobPoint, b: JobPoint): boolean {
  return a.x === b.x && a.y === b.y;
}

/** Whether the fade differs from what the component has. */
export function fadeChanged(next: FadeProp | null, current: FadeProp | null | undefined): boolean {
  const now = current ?? null;
  if (next === null || now === null) {
    return next !== now;
  }
  return (
    !samePoint(next.from, now.from) ||
    !samePoint(next.to, now.to) ||
    Math.abs(next.amount - now.amount) > 1e-9
  );
}

/** Whether the tilt differs from what the component has. */
export function tiltChanged(next: TiltProp | null, current: TiltProp | null | undefined): boolean {
  const now = current ?? null;
  if (next === null || now === null) {
    return next !== now;
  }
  return (
    !samePoint(next.pivot, now.pivot) ||
    !samePoint(next.toward, now.toward) ||
    Math.abs(next.angleDeg - now.angleDeg) > 1e-9
  );
}
