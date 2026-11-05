// Pure helpers for the flat area clearance options shared by the V-Groove
// and inlay forms: offset or raster, raster angle, direction and ramps.

import type { ClearanceOptions, CutDirection } from '../bridge/cam';
import type { Units } from '../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from './formFields';

export interface ClearanceState {
  strategy: ClearanceOptions['strategy'];
  rasterAngle: string;
  direction: CutDirection;
  ramp: boolean;
  rampDistance: string;
}

export type ClearanceField = 'rasterAngle' | 'rampDistance';
export type ClearanceErrors = Partial<Record<ClearanceField, string>>;

export function initialClearance(units: Units): ClearanceState {
  return {
    strategy: 'offset',
    rasterAngle: '0',
    direction: 'climb',
    ramp: false,
    rampDistance: lengthText(units === 'inch' ? 25.4 : 20, units),
  };
}

export function convertClearance(state: ClearanceState, from: Units, to: Units): ClearanceState {
  return convertLengthFields(state, ['rampDistance'], from, to);
}

export function validateClearance(state: ClearanceState, units: Units): ClearanceErrors {
  const errors: ClearanceErrors = {};
  if (state.strategy === 'raster') {
    const error = fieldError(numberValue(state.rasterAngle), { min: -360, max: 360 });
    if (error !== undefined) {
      errors.rasterAngle = error;
    }
  }
  if (state.ramp) {
    const error = fieldError(lengthValue(state.rampDistance, units), { positive: true });
    if (error !== undefined) {
      errors.rampDistance = error;
    }
  }
  return errors;
}

export function clearanceValues(state: ClearanceState, units: Units): ClearanceOptions {
  return {
    strategy: state.strategy,
    rasterAngle: numberValue(state.rasterAngle) ?? 0,
    direction: state.direction,
    ramp: { enabled: state.ramp, distance: lengthValue(state.rampDistance, units) ?? 20 },
  };
}

/** Form state for saved options; toolpaths saved before them have none. */
export function clearanceStateFrom(
  options: ClearanceOptions | undefined,
  units: Units,
): ClearanceState {
  if (options === undefined) {
    return initialClearance(units);
  }
  return {
    strategy: options.strategy,
    rasterAngle: String(options.rasterAngle),
    direction: options.direction,
    ramp: options.ramp.enabled,
    rampDistance: lengthText(options.ramp.distance, units),
  };
}
