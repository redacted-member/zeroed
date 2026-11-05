// Pure state for the Photo Carve toolpath form.

import type { PhotoStrategy, PhotoCarveParams } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

export interface PhotoFormState {
  name: string;
  strategy: PhotoStrategy;
  /** Degrees, -90 to 90, from the slider. */
  lineAngle: number;
  lineSpacing: string;
  startDepth: string;
  maxDepth: string;
  stepoverRetract: string;
}

export type PhotoField = 'lineSpacing' | 'startDepth' | 'maxDepth' | 'stepoverRetract';
export type PhotoErrors = Partial<Record<PhotoField, string>>;

/** Line Spacing in percent of the groove width, from Dense to Sparse. */
export const LINE_SPACING = { min: 10, max: 1000 } as const;

const LENGTHS = ['startDepth', 'maxDepth', 'stepoverRetract'] as const;

export function initialPhotoState(units: Units): PhotoFormState {
  const inch = units === 'inch';
  return {
    name: '',
    strategy: 'raster',
    lineAngle: 45,
    lineSpacing: '100',
    startDepth: lengthText(0, units),
    maxDepth: lengthText(inch ? 2.54 : 2.5, units),
    stepoverRetract: lengthText(inch ? 3.048 : 3, units),
  };
}

export function convertPhotoUnits(state: PhotoFormState, from: Units, to: Units): PhotoFormState {
  return convertLengthFields(state, LENGTHS, from, to);
}

export function validatePhoto(state: PhotoFormState, units: Units): PhotoErrors {
  const errors: PhotoErrors = {};
  const checks: Array<[PhotoField, string | undefined]> = [
    ['lineSpacing', fieldError(numberValue(state.lineSpacing), LINE_SPACING)],
    ['startDepth', fieldError(lengthValue(state.startDepth, units), { min: 0 })],
    ['maxDepth', fieldError(lengthValue(state.maxDepth, units), { positive: true })],
    ['stepoverRetract', fieldError(lengthValue(state.stepoverRetract, units), { min: 0 })],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

export function photoParams(state: PhotoFormState, units: Units): PhotoCarveParams | null {
  if (Object.keys(validatePhoto(state, units)).length > 0) {
    return null;
  }
  return {
    strategy: state.strategy,
    lineAngle: state.lineAngle,
    lineSpacing: numberValue(state.lineSpacing) ?? 100,
    startDepth: lengthValue(state.startDepth, units) ?? 0,
    maxDepth: lengthValue(state.maxDepth, units) ?? 0,
    stepoverRetract: lengthValue(state.stepoverRetract, units) ?? 0,
  };
}

export function photoStateFrom(
  params: PhotoCarveParams,
  name: string,
  units: Units,
): PhotoFormState {
  return {
    name,
    strategy: params.strategy,
    lineAngle: params.lineAngle,
    lineSpacing: String(params.lineSpacing),
    startDepth: lengthText(params.startDepth, units),
    maxDepth: lengthText(params.maxDepth, units),
    stepoverRetract: lengthText(params.stepoverRetract, units),
  };
}
