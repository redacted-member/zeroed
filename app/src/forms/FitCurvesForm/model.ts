// Pure state for the Fit Curves to Vectors form.

import type { FitKind } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import type { FitOptions } from '../../bridge/api';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

export interface FitCurvesFormState {
  kind: FitKind;
  tolerance: string;
  /** Keep Sharp Corners and its Max Angle in degrees. */
  keepSharp: boolean;
  maxAngle: string;
  /** Replace selected vectors: off keeps them and adds the fitted copies. */
  replace: boolean;
}

export function initialFitCurvesState(units: Units): FitCurvesFormState {
  return {
    kind: 'arcs',
    tolerance: lengthText(units === 'inch' ? 0.254 : 0.1, units),
    keepSharp: false,
    maxAngle: '20',
    replace: true,
  };
}

export function maxAngleError(state: FitCurvesFormState): string | undefined {
  return state.keepSharp
    ? fieldError(numberValue(state.maxAngle), { min: 0, max: 180 })
    : undefined;
}

/** The kernel options, or null while Max Angle does not read. */
export function fitOptions(state: FitCurvesFormState): FitOptions | null {
  if (maxAngleError(state) !== undefined) {
    return null;
  }
  const options: FitOptions = { replace: state.replace };
  const angle = numberValue(state.maxAngle);
  if (state.keepSharp && angle !== null) {
    options.sharpAngle = angle;
  }
  return options;
}

export function convertFitCurvesUnits(
  state: FitCurvesFormState,
  from: Units,
  to: Units,
): FitCurvesFormState {
  return convertLengthFields(state, ['tolerance'], from, to);
}

export function fitToleranceError(state: FitCurvesFormState, units: Units): string | undefined {
  return fieldError(lengthValue(state.tolerance, units), { positive: true });
}

/** Tolerance in millimetres, or null while the field is not valid. */
export function fitTolerance(state: FitCurvesFormState, units: Units): number | null {
  return fitToleranceError(state, units) === undefined ? lengthValue(state.tolerance, units) : null;
}
