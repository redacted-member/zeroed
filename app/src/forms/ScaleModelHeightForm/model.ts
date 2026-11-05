// Pure state for the Scale Model Height form: a percentage slider that
// previews live, or an exact height for the model's highest point.

import type { ScaleHeightArgs } from '../../bridge/modelRefine';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';

/** Slider range, percent of the model's height when the form opened. */
export const MIN_PERCENT = 1;
export const MAX_PERCENT = 400;

export interface ScaleModelHeightState {
  percent: number;
  /** Exact height, text in the job unit. */
  height: string;
}

export type ScaleModelHeightErrors = Partial<Record<'height', string>>;

export function initialScaleModelHeightState(
  units: Units,
  maxHeight: number | null,
): ScaleModelHeightState {
  return { percent: 100, height: lengthText(maxHeight ?? 0, units) };
}

export function convertScaleModelHeightUnits(
  state: ScaleModelHeightState,
  from: Units,
  to: Units,
): ScaleModelHeightState {
  return convertLengthFields(state, ['height'], from, to);
}

export function validateScaleModelHeight(
  state: ScaleModelHeightState,
  units: Units,
): ScaleModelHeightErrors {
  const error = fieldError(lengthValue(state.height, units), { positive: true });
  return error === undefined ? {} : { height: error };
}

/** Arguments scaling by the slider's percentage. */
export function percentArgs(
  percent: number,
  replace: number | undefined,
  bothSides = false,
): ScaleHeightArgs {
  const clamped = Math.min(MAX_PERCENT, Math.max(MIN_PERCENT, Math.round(percent)));
  return {
    percent: clamped,
    ...(replace === undefined ? {} : { replace }),
    ...(bothSides ? { bothSides } : {}),
  };
}

/** Arguments setting the exact height (mm), or null when it is not valid. */
export function exactArgs(
  state: ScaleModelHeightState,
  units: Units,
  replace: number | undefined,
  bothSides = false,
): ScaleHeightArgs | null {
  const height = lengthValue(state.height, units);
  if (height === null || validateScaleModelHeight(state, units).height !== undefined) {
    return null;
  }
  return {
    height,
    ...(replace === undefined ? {} : { replace }),
    ...(bothSides ? { bothSides } : {}),
  };
}

/** The slider position a scale factor answered by the kernel stands for. */
export function percentOf(factor: number): number {
  return Math.min(MAX_PERCENT, Math.max(MIN_PERCENT, Math.round(factor * 100)));
}
