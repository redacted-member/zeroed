// Pure state for the Vector Boundary form.

import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';

export interface BoundaryFormState {
  /** Offset Boundary: grow the boundary outwards by `offset`. */
  useOffset: boolean;
  offset: string;
  /** Stretch a band round everything (the convex hull) instead of the outline. */
  rubberBand: boolean;
}

export function initialBoundaryState(units: Units): BoundaryFormState {
  return {
    useOffset: false,
    offset: lengthText(5, units),
    rubberBand: false,
  };
}

export function convertBoundaryUnits(
  state: BoundaryFormState,
  from: Units,
  to: Units,
): BoundaryFormState {
  return convertLengthFields(state, ['offset'], from, to);
}

export function boundaryOffsetError(state: BoundaryFormState, units: Units): string | undefined {
  return state.useOffset ? fieldError(lengthValue(state.offset, units), { min: 0 }) : undefined;
}

/** Offset in millimetres (0 with Offset Boundary off), or null while the field is not valid. */
export function boundaryOffset(state: BoundaryFormState, units: Units): number | null {
  if (!state.useOffset) {
    return 0;
  }
  return boundaryOffsetError(state, units) === undefined ? lengthValue(state.offset, units) : null;
}
