// Pure state for the Fillet form: the radius and the kind of fillet a click
// on a corner applies.

import type { FilletKind } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';

export interface FilletFormState {
  radius: string;
  kind: FilletKind;
}

/** The fillet types in the form's order: the four. */
export const FILLET_KINDS: readonly FilletKind[] = ['normal', 'dogbone', 'tbone', 'plasma'];

export function initialFilletState(units: Units): FilletFormState {
  return { radius: lengthText(units === 'inch' ? 3.175 : 3, units), kind: 'normal' };
}

export function convertFilletUnits(
  state: FilletFormState,
  from: Units,
  to: Units,
): FilletFormState {
  return convertLengthFields(state, ['radius'], from, to);
}

export function radiusError(state: FilletFormState, units: Units): string | undefined {
  return fieldError(lengthValue(state.radius, units), { positive: true });
}

/** Radius in millimetres, or null while the field is not valid. */
export function filletRadius(state: FilletFormState, units: Units): number | null {
  return radiusError(state, units) === undefined ? lengthValue(state.radius, units) : null;
}

/** How near a click must be to a vector, in millimetres, at a given zoom. */
export function pickTolerance(snapRadiusPx: number, pxPerMm: number): number {
  return pxPerMm > 0 ? snapRadiusPx / pxPerMm : 1;
}
