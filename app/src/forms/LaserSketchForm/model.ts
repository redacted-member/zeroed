// Pure state for the Laser Sketch Toolpath form: what to trace, the power
// and overscan, the hatching and the vector selection. An empty power uses
// the laser tool's own.

import type { LaserSketchParams, SketchFill, VectorFilter } from '../../bridge/sketch';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import {
  convertSketchSource,
  initialSketchSource,
  sketchSourceFrom,
  sketchTrace,
  validateSketchSource,
  type SketchSourceErrors,
  type SketchSourceState,
} from '../SketchSourceFields/model';
import { withSelector } from '../VectorSelectionField/model';

export interface LaserSketchState {
  name: string;
  source: SketchSourceState;
  power: string;
  overscan: string;
  stepover: string;
  angle: string;
  crossHatch: boolean;
  fill: SketchFill;
  selector: VectorFilter | null;
}

export type LaserSketchField = 'power' | 'overscan' | 'stepover' | 'angle';
export type LaserSketchErrors = SketchSourceErrors & Partial<Record<LaserSketchField, string>>;

export function initialLaserSketchState(units: Units): LaserSketchState {
  return {
    name: '',
    source: initialSketchSource(units),
    power: '',
    overscan: lengthText(0, units),
    stepover: lengthText(units === 'inch' ? 0.127 : 0.1, units),
    angle: '0',
    crossHatch: false,
    fill: 'together',
    selector: null,
  };
}

export function convertLaserSketchUnits(
  state: LaserSketchState,
  from: Units,
  to: Units,
): LaserSketchState {
  const next = convertLengthFields(state, ['overscan', 'stepover'], from, to);
  return { ...next, source: convertSketchSource(state.source, from, to) };
}

export function validateLaserSketch(
  state: LaserSketchState,
  units: Units,
  boundaryCount: number,
): LaserSketchErrors {
  const errors: LaserSketchErrors = validateSketchSource(state.source, units, boundaryCount);
  const checks: Array<[LaserSketchField, string | undefined]> = [
    [
      'power',
      state.power.trim() === ''
        ? undefined
        : fieldError(numberValue(state.power), { min: 0, max: 100 }),
    ],
    ['overscan', fieldError(lengthValue(state.overscan, units), { min: 0 })],
    ['stepover', fieldError(lengthValue(state.stepover, units), { min: 0.01 })],
    ['angle', fieldError(numberValue(state.angle), { min: -360, max: 360 })],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

export function laserSketchParams(
  state: LaserSketchState,
  units: Units,
  boundaryCount: number,
): LaserSketchParams | null {
  if (Object.keys(validateLaserSketch(state, units, boundaryCount)).length > 0) {
    return null;
  }
  const trace = sketchTrace(state.source, units, boundaryCount);
  if (trace === null) {
    return null;
  }
  return withSelector(
    {
      ...trace,
      power: state.power.trim() === '' ? null : numberValue(state.power),
      speed: null,
      overscan: lengthValue(state.overscan, units) ?? 0,
      stepover: lengthValue(state.stepover, units) ?? 0.1,
      angle: numberValue(state.angle) ?? 0,
      crossHatch: state.crossHatch,
      fill: state.fill,
    },
    state.selector,
  );
}

export function laserSketchStateFrom(
  params: LaserSketchParams,
  name: string,
  units: Units,
): LaserSketchState {
  return {
    name,
    source: sketchSourceFrom(params, units),
    power: params.power === null ? '' : String(params.power),
    overscan: lengthText(params.overscan, units),
    stepover: lengthText(params.stepover, units),
    angle: String(params.angle),
    crossHatch: params.crossHatch,
    fill: params.fill,
    selector: params.selector ?? null,
  };
}
