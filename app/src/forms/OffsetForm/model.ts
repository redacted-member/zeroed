// Pure state for the Offset Vectors form.

import type { OffsetArgs } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';

export type OffsetSide = OffsetArgs['side'];
export type OffsetCorner = OffsetArgs['corner'];

export interface OffsetFormState {
  distance: string;
  side: OffsetSide;
  corner: OffsetCorner;
}

export function initialOffsetState(units: Units): OffsetFormState {
  return {
    distance: lengthText(units === 'inch' ? 6.35 : 5, units),
    side: 'outward',
    corner: 'round',
  };
}

export function convertOffsetUnits(
  state: OffsetFormState,
  from: Units,
  to: Units,
): OffsetFormState {
  return convertLengthFields(state, ['distance'], from, to);
}

export function distanceError(state: OffsetFormState, units: Units): string | undefined {
  return fieldError(lengthValue(state.distance, units), { positive: true });
}

/** Kernel arguments without the ids, or null while the distance is invalid. */
export function offsetArgs(state: OffsetFormState, units: Units): Omit<OffsetArgs, 'ids'> | null {
  const distance = lengthValue(state.distance, units);
  if (distance === null || distanceError(state, units) !== undefined) {
    return null;
  }
  return {
    distance,
    side: state.side,
    corner: state.corner,
    deleteOriginal: false,
  };
}
