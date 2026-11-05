// Pure state for the Fluting Toolpath form.

import type { FlutingParams, FlutingRamp } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';

/** Where the flute ramps: over its whole length from the start depth to
 * the flute depth, at the start only, or at both ends. */
export type FluteType = 'complete' | 'start' | 'startEnd';
/** How a ramp's length is given. */
export type RampBy = 'length' | 'percent';

export interface FlutingFormState {
  name: string;
  selectionOrder: boolean;
  startDepth: string;
  depth: string;
  fluteType: FluteType;
  ramp: FlutingRamp;
  rampBy: RampBy;
  rampLength: string;
  rampPercent: string;
}

export type FlutingField = 'startDepth' | 'depth' | 'rampLength' | 'rampPercent';
export type FlutingErrors = Partial<Record<FlutingField, string>>;

export function initialFlutingState(units: Units): FlutingFormState {
  const inch = units === 'inch';
  return {
    selectionOrder: false,
    name: '',
    startDepth: lengthText(0, units),
    depth: lengthText(inch ? 3.175 : 3, units),
    fluteType: 'complete',
    ramp: 'linear',
    rampBy: 'percent',
    rampLength: lengthText(inch ? 12.7 : 10, units),
    rampPercent: '100',
  };
}

export function convertFlutingUnits(
  state: FlutingFormState,
  from: Units,
  to: Units,
): FlutingFormState {
  return convertLengthFields(state, ['startDepth', 'depth', 'rampLength'], from, to);
}

/** The ramp size fields matter for a ramp at the start or at both ends. */
export function usesRampSize(state: FlutingFormState): boolean {
  return state.fluteType !== 'complete';
}

export function validateFluting(state: FlutingFormState, units: Units): FlutingErrors {
  const errors: FlutingErrors = {};
  const start = fieldError(lengthValue(state.startDepth, units), { min: 0 });
  if (start !== undefined) {
    errors.startDepth = start;
  }
  const depth = fieldError(lengthValue(state.depth, units), { positive: true });
  if (depth !== undefined) {
    errors.depth = depth;
  }
  if (usesRampSize(state) && state.rampBy === 'length') {
    const length = fieldError(lengthValue(state.rampLength, units), { positive: true });
    if (length !== undefined) {
      errors.rampLength = length;
    }
  }
  if (usesRampSize(state) && state.rampBy === 'percent') {
    const raw = state.rampPercent.trim() === '' ? NaN : Number(state.rampPercent);
    const percent = fieldError(Number.isFinite(raw) ? raw : null, { positive: true, max: 100 });
    if (percent !== undefined) {
      errors.rampPercent = percent;
    }
  }
  return errors;
}

export function flutingParams(state: FlutingFormState, units: Units): FlutingParams | null {
  if (Object.keys(validateFluting(state, units)).length > 0) {
    return null;
  }
  const sized = usesRampSize(state);
  return {
    selectionOrder: state.selectionOrder,
    startDepth: lengthValue(state.startDepth, units) ?? 0,
    depth: lengthValue(state.depth, units) ?? 3,
    ramp: state.ramp,
    rampStart: true,
    rampEnd: state.fluteType === 'startEnd',
    rampLength: sized && state.rampBy === 'length' ? lengthValue(state.rampLength, units) : null,
    rampPercent: sized && state.rampBy === 'percent' ? Number(state.rampPercent) : null,
  };
}

/** The flute type an older toolpath's ramp ends come closest to. */
function fluteTypeOf(params: FlutingParams): FluteType {
  if (params.rampStart && params.rampEnd) {
    return 'startEnd';
  }
  const sized = params.rampLength !== null || (params.rampPercent ?? null) !== null;
  return sized ? 'start' : 'complete';
}

export function flutingStateFrom(
  params: FlutingParams,
  name: string,
  units: Units,
): FlutingFormState {
  const base = initialFlutingState(units);
  const percent = params.rampPercent ?? null;
  const fluteType = fluteTypeOf(params);
  // A ramp over the whole vector is 100 percent of it.
  const rampBy: RampBy = params.rampLength !== null ? 'length' : 'percent';
  return {
    name,
    selectionOrder: params.selectionOrder ?? false,
    startDepth: lengthText(params.startDepth ?? 0, units),
    depth: lengthText(params.depth, units),
    fluteType,
    ramp: params.ramp,
    rampBy,
    rampLength: params.rampLength === null ? base.rampLength : lengthText(params.rampLength, units),
    rampPercent: percent === null ? base.rampPercent : String(percent),
  };
}
