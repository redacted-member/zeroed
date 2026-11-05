// Pure state for the Offset Model form: the 3D offset distance and whether
// the result is kept above the zero plane. Previews add up; OK keeps them,
// Cancel takes them all back.

import type { OffsetModelArgs } from '../../bridge/modelRefine';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';

export interface OffsetModelState {
  /** Text in the job unit; negative shrinks the model. */
  distance: string;
  clipToZero: boolean;
}

export type OffsetModelErrors = Partial<Record<'distance', string>>;

/** The previewed offset still up: its step's revision and component. */
export interface OffsetPreview {
  revision: number;
  id: number;
}

export function initialOffsetModelState(units: Units): OffsetModelState {
  return { distance: lengthText(units === 'inch' ? 1.27 : 1, units), clipToZero: false };
}

export function convertOffsetModelUnits(
  state: OffsetModelState,
  from: Units,
  to: Units,
): OffsetModelState {
  return convertLengthFields(state, ['distance'], from, to);
}

export function validateOffsetModel(state: OffsetModelState, units: Units): OffsetModelErrors {
  const distance = lengthValue(state.distance, units);
  const error =
    fieldError(distance) ?? (distance === 0 ? strings.model.refine.offset.notZero : undefined);
  return error === undefined ? {} : { distance: error };
}

/** The model.offsetModel arguments, adding onto `previous` when one is up. */
export function offsetArgs(
  state: OffsetModelState,
  units: Units,
  previous: OffsetPreview | null,
): OffsetModelArgs | null {
  const distance = lengthValue(state.distance, units);
  if (distance === null || validateOffsetModel(state, units).distance !== undefined) {
    return null;
  }
  return {
    distance,
    clipToZero: state.clipToZero,
    ...(previous === null ? {} : { merge: previous }),
  };
}
