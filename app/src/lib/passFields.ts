// Pure helpers for the pass options shared by the Profile and Pocket forms:
// passes by the pass depth (stretched by up to 15% to save a pass unless
// the exact depth is kept) or by count, an optional last pass thickness,
// and a custom list of pass depths from the Edit Passes dialog.

import type { Units } from '../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from './formFields';

/** How far the pass depth may grow to save a pass, as the kernel allows. */
export const PASS_FLEX = 0.15;

export interface PassState {
  /** Evenly spaced passes by count instead of by the pass depth. */
  byCount: boolean;
  passCount: string;
  useLastPass: boolean;
  lastPassThickness: string;
  /** Maintain Exact Step Depth. */
  exact: boolean;
  /** Custom pass depths in millimetres below the Start Depth, or null. */
  list: number[] | null;
}

export type PassField = 'passCount' | 'lastPassThickness';
export type PassErrors = Partial<Record<PassField, string>>;

export interface PassValues {
  passCount: number | null;
  lastPassThickness: number | null;
  exactPassDepth: boolean;
  passList: number[] | null;
}

export function initialPasses(units: Units): PassState {
  return {
    byCount: false,
    passCount: '2',
    useLastPass: false,
    lastPassThickness: lengthText(units === 'inch' ? 0.508 : 0.5, units),
    exact: false,
    list: null,
  };
}

export function convertPasses(state: PassState, from: Units, to: Units): PassState {
  return convertLengthFields(state, ['lastPassThickness'], from, to);
}

export function validatePasses(state: PassState, units: Units): PassErrors {
  const errors: PassErrors = {};
  if (state.list !== null) {
    return errors;
  }
  if (state.byCount) {
    const error = fieldError(numberValue(state.passCount), { min: 1, max: 1000, integer: true });
    if (error !== undefined) {
      errors.passCount = error;
    }
  }
  if (state.useLastPass) {
    const error = fieldError(lengthValue(state.lastPassThickness, units), { positive: true });
    if (error !== undefined) {
      errors.lastPassThickness = error;
    }
  }
  return errors;
}

/** Kernel values; nulls leave the choice to the pass depth. `startDepth` in millimetres. */
export function passValues(state: PassState, units: Units, startDepth = 0): PassValues {
  return {
    passCount: state.byCount ? (numberValue(state.passCount) ?? null) : null,
    lastPassThickness: state.useLastPass
      ? (lengthValue(state.lastPassThickness, units) ?? null)
      : null,
    exactPassDepth: state.exact,
    passList: state.list === null ? null : state.list.map((d) => startDepth + d),
  };
}

/** Form state for saved values; toolpaths saved before these existed have none. */
export function passStateFrom(
  passCount: number | null | undefined,
  lastPassThickness: number | null | undefined,
  units: Units,
  extra: { exact?: boolean; list?: number[] | null; startDepth?: number } = {},
): PassState {
  const base = initialPasses(units);
  const start = extra.startDepth ?? 0;
  return {
    byCount: passCount !== null && passCount !== undefined,
    passCount: passCount === null || passCount === undefined ? base.passCount : String(passCount),
    useLastPass: lastPassThickness !== null && lastPassThickness !== undefined,
    lastPassThickness:
      lastPassThickness === null || lastPassThickness === undefined
        ? base.lastPassThickness
        : lengthText(lastPassThickness, units),
    exact: extra.exact === true,
    list: extra.list === null || extra.list === undefined ? null : extra.list.map((d) => d - start),
  };
}

function even(start: number, cut: number, count: number): number[] {
  const span = cut - start;
  return Array.from({ length: count }, (_, i) =>
    i === count - 1 ? cut : start + (span * (i + 1)) / count,
  );
}

/**
 * Depths by the pass depth, as the kernel's stepped_depths: exact steps with
 * the last pass taking what is left, or even passes, fewer when the pass
 * depth may grow by up to 15% to save one. Null for depths that make no sense.
 */
export function steppedDepths(
  start: number,
  cut: number,
  passDepth: number,
  exact: boolean,
): number[] | null {
  if (!(cut > start) || !(passDepth > 0) || !Number.isFinite(cut) || !Number.isFinite(start)) {
    return null;
  }
  const span = cut - start;
  const count = Math.max(1, Math.ceil(span / passDepth - 1e-9));
  if (exact) {
    const depths: number[] = [];
    for (let i = 1; i < count; i++) {
      const d = start + passDepth * i;
      if (d < cut - 1e-9) {
        depths.push(d);
      }
    }
    depths.push(cut);
    return depths;
  }
  const fewer = Math.max(1, Math.ceil(span / (passDepth * (1 + PASS_FLEX)) - 1e-9));
  return even(start, cut, Math.min(fewer, count));
}

export interface PassPlanValues {
  passCount: number | null;
  lastPassThickness: number | null;
  exactPassDepth: boolean;
  passList: number[] | null;
}

/** The pass depths the kernel will cut (millimetres below the surface), as its planned_depths. */
export function plannedDepths(
  start: number,
  cut: number,
  passDepth: number,
  plan: PassPlanValues,
): number[] | null {
  const stepped = steppedDepths(start, cut, passDepth, plan.exactPassDepth);
  if (stepped === null) {
    return null;
  }
  if (plan.passList !== null) {
    const custom = plan.passList
      .filter((d) => d > start + 1e-9 && d < cut - 1e-9)
      .sort((a, b) => a - b)
      .filter((d, i, all) => i === 0 || Math.abs(d - (all[i - 1] ?? d)) >= 1e-9);
    return [...custom, cut];
  }
  if (plan.passCount === 0) {
    return null;
  }
  const t = plan.lastPassThickness;
  const last = t !== null && t > 1e-9 ? t : null;
  if (last !== null && last >= cut - start - 1e-9) {
    return null;
  }
  const above = cut - (last ?? 0);
  let depths = stepped;
  if (plan.passCount !== null) {
    const n = last !== null ? Math.max(1, plan.passCount - 1) : plan.passCount;
    depths = even(start, above, n);
  } else if (last !== null) {
    depths = steppedDepths(start, above, passDepth, plan.exactPassDepth) ?? [];
  }
  return last !== null ? [...depths, cut] : depths;
}
