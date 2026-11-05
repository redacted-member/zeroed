// Pure state for the Quick Engrave Toolpath form.

import type { EngraveFill, EngraveParams } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

export interface EngraveFormState {
  name: string;
  depth: string;
  passes: string;
  mode: EngraveParams['mode'];
  stepover: string;
  angle: string;
  pattern: EngraveFill;
  noseCone: boolean;
  /** How far the tool stands out of the nose cone: the depth of every pass. */
  toolDepth: string;
}

export type EngraveField = 'depth' | 'toolDepth' | 'passes' | 'stepover' | 'angle';
export type EngraveErrors = Partial<Record<EngraveField, string>>;

export function initialEngraveState(units: Units): EngraveFormState {
  return {
    name: '',
    depth: lengthText(units === 'inch' ? 0.508 : 0.5, units),
    passes: '1',
    mode: 'outline',
    stepover: lengthText(units === 'inch' ? 0.254 : 0.3, units),
    angle: '0',
    pattern: 'raster',
    noseCone: false,
    toolDepth: lengthText(units === 'inch' ? 0.508 : 0.5, units),
  };
}

export function convertEngraveUnits(
  state: EngraveFormState,
  from: Units,
  to: Units,
): EngraveFormState {
  return convertLengthFields(state, ['depth', 'stepover', 'toolDepth'], from, to);
}

export function validateEngrave(state: EngraveFormState, units: Units): EngraveErrors {
  const errors: EngraveErrors = {};
  const checks: Array<[EngraveField, string | undefined]> = [
    ['passes', fieldError(numberValue(state.passes), { min: 1, max: 100, integer: true })],
  ];
  if (state.noseCone) {
    checks.push(['toolDepth', fieldError(lengthValue(state.toolDepth, units), { positive: true })]);
  } else {
    checks.push(['depth', fieldError(lengthValue(state.depth, units), { positive: true })]);
  }
  if (state.mode === 'fill') {
    checks.push(
      ['stepover', fieldError(lengthValue(state.stepover, units), { positive: true })],
      ['angle', fieldError(numberValue(state.angle), { min: -360, max: 360 })],
    );
  }
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

export function engraveParams(state: EngraveFormState, units: Units): EngraveParams | null {
  if (Object.keys(validateEngrave(state, units)).length > 0) {
    return null;
  }
  return {
    depth: lengthValue(state.noseCone ? state.toolDepth : state.depth, units) ?? 0,
    passes: numberValue(state.passes) ?? 1,
    mode: state.mode,
    stepover: lengthValue(state.stepover, units) ?? 0.3,
    angle: numberValue(state.angle) ?? 0,
    pattern: state.pattern,
    noseCone: state.noseCone,
  };
}

export function engraveStateFrom(
  params: EngraveParams,
  name: string,
  units: Units,
): EngraveFormState {
  const base = initialEngraveState(units);
  const noseCone = params.noseCone ?? false;
  return {
    name,
    depth: noseCone ? base.depth : lengthText(params.depth, units),
    passes: String(params.passes),
    mode: params.mode,
    stepover: lengthText(params.stepover, units),
    angle: String(params.angle),
    pattern: params.pattern ?? 'raster',
    noseCone,
    toolDepth: noseCone ? lengthText(params.depth, units) : base.toolDepth,
  };
}
