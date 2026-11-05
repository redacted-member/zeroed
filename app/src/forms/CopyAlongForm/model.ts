// Pure state for the Copy Along Vectors form.

import type { CopyAlongArgs } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

/** Copy Object copies the first selected object; Copy Circles places
 * circles. */
export type CopyWhat = 'object' | 'circles';

export interface CopyAlongFormState {
  what: CopyWhat;
  diameter: string;
  distributeMode: CopyAlongArgs['distributeMode'];
  count: string;
  spacing: string;
  forceEven: boolean;
  newLayer: boolean;
  alignToCurve: boolean;
  reverse: boolean;
}

export type CopyAlongField = 'count' | 'spacing' | 'diameter';
export type CopyAlongErrors = Partial<Record<CopyAlongField, string>>;

export function initialCopyAlongState(units: Units): CopyAlongFormState {
  return {
    what: 'object',
    diameter: lengthText(units === 'inch' ? 6.35 : 6, units),
    distributeMode: 'count',
    count: '5',
    spacing: lengthText(units === 'inch' ? 25.4 : 20, units),
    forceEven: false,
    newLayer: false,
    alignToCurve: true,
    reverse: false,
  };
}

export function convertCopyAlongUnits(
  state: CopyAlongFormState,
  from: Units,
  to: Units,
): CopyAlongFormState {
  return convertLengthFields(state, ['spacing', 'diameter'], from, to);
}

export function validateCopyAlong(state: CopyAlongFormState, units: Units): CopyAlongErrors {
  const errors: CopyAlongErrors = {};
  if (state.distributeMode === 'count') {
    const count = fieldError(numberValue(state.count), { min: 1, max: 1000, integer: true });
    if (count !== undefined) {
      errors.count = count;
    }
  } else {
    const spacing = fieldError(lengthValue(state.spacing, units), { positive: true });
    if (spacing !== undefined) {
      errors.spacing = spacing;
    }
  }
  if (state.what === 'circles') {
    const diameter = fieldError(lengthValue(state.diameter, units), { positive: true });
    if (diameter !== undefined) {
      errors.diameter = diameter;
    }
  }
  return errors;
}

/** Kernel arguments for the guide vectors, or null while a field is invalid. */
export function copyAlongArgs(
  state: CopyAlongFormState,
  units: Units,
  pathIds: number[],
): CopyAlongArgs | null {
  if (Object.keys(validateCopyAlong(state, units)).length > 0) {
    return null;
  }
  const args: CopyAlongArgs = {
    pathIds,
    distributeMode: state.distributeMode,
    count: numberValue(state.count) ?? 0,
    spacing: lengthValue(state.spacing, units) ?? 0,
    forceEven: state.forceEven,
    newLayer: state.newLayer,
    alignToCurve: state.alignToCurve,
    startOffset: 0,
    reverse: state.reverse,
    deleteOriginal: false,
  };
  if (state.what === 'circles') {
    args.circleDiameter = lengthValue(state.diameter, units) ?? 0;
  }
  return args;
}

/** Which selected vectors are copied and which are the curves, as the
 * reference reads the selection: the first selected object is copied along
 * every other one. Copy Circles, or a picked component as the object, makes
 * every selected vector a curve. */
export function copyAlongRoles(
  ids: readonly number[],
  what: CopyWhat,
  components: number,
): { objects: number[]; curves: number[] } {
  if (what === 'circles' || components > 0) {
    return { objects: [], curves: [...ids] };
  }
  return { objects: ids.slice(0, 1), curves: ids.slice(1) };
}

/** True when there is something to copy and a curve to copy it along. */
export function copyAlongReady(
  ids: readonly number[],
  what: CopyWhat,
  components: number,
): boolean {
  const { objects, curves } = copyAlongRoles(ids, what, components);
  return curves.length > 0 && (what === 'circles' || objects.length > 0 || components > 0);
}
