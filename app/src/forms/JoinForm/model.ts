// Pure state for the Join Vectors form: the gap two ends may have and still
// be joined.

import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';

export interface JoinFormState {
  tolerance: string;
}

export function initialJoinState(units: Units): JoinFormState {
  return { tolerance: lengthText(0.1, units) };
}

export function convertJoinUnits(state: JoinFormState, from: Units, to: Units): JoinFormState {
  return convertLengthFields(state, ['tolerance'], from, to);
}

export function toleranceError(state: JoinFormState, units: Units): string | undefined {
  return fieldError(lengthValue(state.tolerance, units), { positive: true });
}

/** Tolerance in millimetres, or null when the field is not valid. */
export function joinTolerance(state: JoinFormState, units: Units): number | null {
  return toleranceError(state, units) === undefined ? lengthValue(state.tolerance, units) : null;
}
