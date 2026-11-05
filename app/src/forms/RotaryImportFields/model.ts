// Pure state for importing a full 3D model into a rotary job: where the
// rotation axis runs through the model and the size of the cylinder round
// it (length and diameter), with the ratio locked or not.

import type { Cylinder, RotaryFit } from '../../bridge/meshImport';
import type { Units } from '../../bridge/types';
import { fieldError, lengthText, lengthValue } from '../../lib/formFields';

export interface RotaryFitState {
  /** Display unit. */
  length: string;
  /** Display unit. */
  diameter: string;
  lockRatio: boolean;
  /** Axis offset across the model, display unit. */
  side: string;
  /** Axis offset up the model, display unit. */
  up: string;
}

export function rotaryFitState(
  fit: RotaryFit,
  cylinder: Cylinder | null,
  units: Units,
): RotaryFitState {
  return {
    length: lengthText(cylinder?.length ?? 0, units),
    diameter: lengthText(cylinder?.diameter ?? 0, units),
    lockRatio: true,
    side: lengthText(fit.offset[0], units),
    up: lengthText(fit.offset[1], units),
  };
}

/** The fields for a new cylinder from the kernel, keeping the rest. */
export function withCylinder(
  state: RotaryFitState,
  cylinder: Cylinder,
  units: Units,
): RotaryFitState {
  return {
    ...state,
    length: lengthText(cylinder.length, units),
    diameter: lengthText(cylinder.diameter, units),
  };
}

/** The axis offset fields for an offset set elsewhere (panning). */
export function withOffset(
  state: RotaryFitState,
  offset: [number, number],
  units: Units,
): RotaryFitState {
  return { ...state, side: lengthText(offset[0], units), up: lengthText(offset[1], units) };
}

export type RotaryField = 'length' | 'diameter' | 'side' | 'up';

export function rotaryErrors(
  state: RotaryFitState,
  units: Units,
): Partial<Record<RotaryField, string>> {
  const errors: Partial<Record<RotaryField, string>> = {};
  const checks: Array<[RotaryField, string | undefined]> = [
    ['length', fieldError(lengthValue(state.length, units), { positive: true })],
    ['diameter', fieldError(lengthValue(state.diameter, units), { positive: true })],
    ['side', fieldError(lengthValue(state.side, units))],
    ['up', fieldError(lengthValue(state.up, units))],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** A typed length or diameter; with the ratio locked the other follows. */
export function typeSize(
  state: RotaryFitState,
  field: 'length' | 'diameter',
  text: string,
  cylinder: Cylinder | null,
  units: Units,
): RotaryFitState {
  const next = { ...state, [field]: text };
  const typed = lengthValue(text, units);
  if (!state.lockRatio || cylinder === null || typed === null || typed <= 0) {
    return next;
  }
  const now = field === 'length' ? cylinder.length : cylinder.diameter;
  if (now <= 0) {
    return next;
  }
  const k = typed / now;
  return field === 'length'
    ? { ...next, diameter: lengthText(cylinder.diameter * k, units) }
    : { ...next, length: lengthText(cylinder.length * k, units) };
}

/** The fit the fields ask for, from the fit and cylinder they were read
 * from; null while a field is wrong. */
export function fitFromState(
  state: RotaryFitState,
  fit: RotaryFit,
  cylinder: Cylinder | null,
  units: Units,
): RotaryFit | null {
  if (Object.keys(rotaryErrors(state, units)).length > 0) {
    return null;
  }
  const length = lengthValue(state.length, units) ?? 0;
  const diameter = lengthValue(state.diameter, units) ?? 0;
  const along =
    cylinder !== null && cylinder.length > 0
      ? (fit.alongScale * length) / cylinder.length
      : fit.alongScale;
  const radial =
    cylinder !== null && cylinder.diameter > 0
      ? (fit.radialScale * diameter) / cylinder.diameter
      : fit.radialScale;
  return {
    offset: [lengthValue(state.side, units) ?? 0, lengthValue(state.up, units) ?? 0],
    alongScale: along,
    radialScale: radial,
  };
}

/** Scale Model to Fit Material: as big as the blank allows, keeping the
 * ratio when it is locked. */
export function fitToBlank(
  fit: RotaryFit,
  cylinder: Cylinder,
  blank: Cylinder,
  lockRatio: boolean,
): RotaryFit {
  if (cylinder.length <= 0 || cylinder.diameter <= 0) {
    return fit;
  }
  const kl = blank.length / cylinder.length;
  const kd = blank.diameter / cylinder.diameter;
  const [a, r] = lockRatio ? [Math.min(kl, kd), Math.min(kl, kd)] : [kl, kd];
  return { ...fit, alongScale: fit.alongScale * a, radialScale: fit.radialScale * r };
}

/** Reset to Original Size: no scaling (the model units stay). */
export function originalSize(fit: RotaryFit): RotaryFit {
  return { ...fit, alongScale: 1, radialScale: 1 };
}
