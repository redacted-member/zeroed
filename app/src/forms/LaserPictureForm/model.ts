// Pure state for the Laser Picture Toolpath form: the bitmap selected in the
// drawing is burnt along raster or hatch lines, or along the selected
// vectors, with the power following its grey levels or dithered.

import type { LaserPictureParams, PictureStrategy } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

export interface LaserPictureState {
  name: string;
  strategy: PictureStrategy;
  /** Dither Image: dots fully on or off instead of varying power. */
  ditherImage: boolean;
  lineSpacing: string;
  /** Line Angle, -90 to 90 degrees. */
  lineAngle: string;
  power: string;
  speed: string;
  /** No. Passes. */
  repeats: string;
  /** Overscan, percent of each line. */
  overscan: string;
  /** An older toolpath's image file and placement, kept as they were. */
  legacy: Pick<LaserPictureParams, 'path' | 'origin' | 'width' | 'height'> | null;
}

export type LaserPictureField =
  'lineSpacing' | 'lineAngle' | 'power' | 'speed' | 'repeats' | 'overscan';
export type LaserPictureErrors = Partial<Record<LaserPictureField, string>>;

const LENGTHS: Array<'lineSpacing' | 'speed'> = ['lineSpacing', 'speed'];

/** The Line Spacing slider's range in the job unit, dense to sparse. */
export function lineSpacingRange(units: Units): { min: number; max: number; step: number } {
  return units === 'inch'
    ? { min: 0.002, max: 0.04, step: 0.001 }
    : { min: 0.05, max: 1, step: 0.01 };
}

export function initialLaserPictureState(units: Units): LaserPictureState {
  return {
    name: '',
    strategy: 'raster',
    ditherImage: false,
    lineSpacing: lengthText(units === 'inch' ? 0.127 : 0.1, units),
    lineAngle: '0',
    power: '',
    speed: '',
    repeats: '1',
    overscan: '0',
    legacy: null,
  };
}

export function convertLaserPictureUnits(
  state: LaserPictureState,
  from: Units,
  to: Units,
): LaserPictureState {
  return convertLengthFields(state, LENGTHS, from, to);
}

export function validateLaserPicture(state: LaserPictureState, units: Units): LaserPictureErrors {
  const errors: LaserPictureErrors = {};
  const checks: Array<[LaserPictureField, string | undefined]> = [
    ['lineSpacing', fieldError(lengthValue(state.lineSpacing, units), { min: 0.01 })],
    ['lineAngle', fieldError(numberValue(state.lineAngle), { min: -90, max: 90 })],
    [
      'power',
      state.power.trim() === ''
        ? undefined
        : fieldError(numberValue(state.power), { min: 0, max: 100 }),
    ],
    [
      'speed',
      state.speed.trim() === ''
        ? undefined
        : fieldError(lengthValue(state.speed, units), { positive: true }),
    ],
    ['repeats', fieldError(numberValue(state.repeats), { min: 1, max: 100, integer: true })],
    ['overscan', fieldError(numberValue(state.overscan), { min: 0, max: 100 })],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** The kernel settings, or null while a field is invalid. */
export function laserPictureParams(
  state: LaserPictureState,
  units: Units,
): LaserPictureParams | null {
  if (Object.keys(validateLaserPicture(state, units)).length > 0) {
    return null;
  }
  return {
    strategy: state.strategy,
    lineAngle: numberValue(state.lineAngle) ?? 0,
    repeats: numberValue(state.repeats) ?? 1,
    overscanPercent: numberValue(state.overscan) ?? 0,
    path: state.legacy?.path ?? '',
    origin: state.legacy?.origin ?? { x: 0, y: 0 },
    width: state.legacy?.width ?? 0,
    height: state.legacy?.height ?? 0,
    lineInterval: lengthValue(state.lineSpacing, units) ?? 0.1,
    mode: state.ditherImage ? 'dither' : 'powerModulation',
    dither: 'floydSteinberg',
    minPower: 0,
    maxPower: state.power.trim() === '' ? null : numberValue(state.power),
    speed: state.speed.trim() === '' ? null : lengthValue(state.speed, units),
    invert: false,
    bidirectional: true,
    overscan: 0,
  };
}

export function laserPictureStateFrom(
  params: LaserPictureParams,
  name: string,
  units: Units,
): LaserPictureState {
  return {
    name,
    strategy: params.strategy ?? 'raster',
    ditherImage: params.mode === 'dither',
    lineSpacing: lengthText(params.lineInterval, units),
    lineAngle: String(params.lineAngle ?? 0),
    power: params.maxPower === null ? '' : String(params.maxPower),
    speed: params.speed === null ? '' : lengthText(params.speed, units),
    repeats: String(params.repeats ?? 1),
    overscan: String(params.overscanPercent ?? 0),
    legacy: burnsAFile(params)
      ? { path: params.path, origin: params.origin, width: params.width, height: params.height }
      : null,
  };
}

/** An older toolpath burnt an image file rather than a selected bitmap. */
export function burnsAFile(params: LaserPictureParams): boolean {
  return params.path !== '';
}
