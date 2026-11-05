// Pure state for the Texture Toolpath form, in the terms:
// maximum depth and stroke length with minimums as percentages of them, an
// overlap between strokes with its variation, stepover and angle.

import type { TextureParams } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

export interface TextureFormState {
  name: string;
  startDepth: string;
  depth: string;
  minDepth: string;
  strokeLength: string;
  minLength: string;
  maxOverlap: string;
  overlapVariation: string;
  spacing: string;
  /** Percent (0 to 50) of the stepover a stroke may move across its row. */
  stepoverVariation: string;
  angle: string;
  boundaryOffset: string;
  vectorsAsPattern: boolean;
  ramp: boolean;
  /** The pattern's seed: the form has no control for it, so a toolpath keeps its own. */
  seed: number;
  /** Kept from toolpaths made before the form lost the field. */
  angleVariation: number;
}

export type TextureField = Exclude<
  keyof TextureFormState,
  'name' | 'vectorsAsPattern' | 'ramp' | 'angleVariation' | 'seed'
>;
export type TextureErrors = Partial<Record<TextureField, string>>;

/** The stepover variation slider's range, percent of the stepover either way. */
export const MAX_STEPOVER_VARIATION = 50;

const LENGTH_FIELDS = ['startDepth', 'strokeLength', 'spacing', 'depth', 'boundaryOffset'] as const;

/** Marks a file of saved texture settings. */
const SETTINGS_FORMAT = 'carve-texture-settings';

function percentText(fraction: number): string {
  return String(Math.round(fraction * 100 * 1000) / 1000);
}

export function initialTextureState(units: Units): TextureFormState {
  const inch = units === 'inch';
  return {
    name: '',
    startDepth: lengthText(0, units),
    depth: lengthText(inch ? 3.175 : 4, units),
    minDepth: '50',
    strokeLength: lengthText(inch ? 9.525 : 10, units),
    minLength: '50',
    maxOverlap: '30',
    overlapVariation: '50',
    spacing: lengthText(inch ? 3.175 : 3, units),
    stepoverVariation: '25',
    angle: '0',
    boundaryOffset: lengthText(0, units),
    vectorsAsPattern: false,
    ramp: true,
    seed: 1,
    angleVariation: 0,
  };
}

export function convertTextureUnits(
  state: TextureFormState,
  from: Units,
  to: Units,
): TextureFormState {
  return convertLengthFields(state, LENGTH_FIELDS, from, to);
}

export function validateTexture(state: TextureFormState, units: Units): TextureErrors {
  const errors: TextureErrors = {};
  const scatter = !state.vectorsAsPattern;
  const checks: Array<[TextureField, string | undefined]> = [
    ['startDepth', fieldError(lengthValue(state.startDepth, units), { min: 0 })],
    ['depth', fieldError(lengthValue(state.depth, units), { positive: true })],
    ['minDepth', fieldError(numberValue(state.minDepth), { positive: true, max: 100 })],
    [
      'strokeLength',
      scatter ? fieldError(lengthValue(state.strokeLength, units), { positive: true }) : undefined,
    ],
    [
      'minLength',
      scatter ? fieldError(numberValue(state.minLength), { positive: true, max: 100 }) : undefined,
    ],
    [
      'maxOverlap',
      scatter ? fieldError(numberValue(state.maxOverlap), { min: 0, max: 100 }) : undefined,
    ],
    [
      'overlapVariation',
      scatter ? fieldError(numberValue(state.overlapVariation), { min: 0, max: 100 }) : undefined,
    ],
    [
      'spacing',
      scatter ? fieldError(lengthValue(state.spacing, units), { positive: true }) : undefined,
    ],
    [
      'stepoverVariation',
      scatter
        ? fieldError(numberValue(state.stepoverVariation), {
            min: 0,
            max: MAX_STEPOVER_VARIATION,
          })
        : undefined,
    ],
    ['angle', scatter ? fieldError(numberValue(state.angle), { min: 0, max: 90 }) : undefined],
    [
      'boundaryOffset',
      scatter ? fieldError(lengthValue(state.boundaryOffset, units), { min: 0 }) : undefined,
    ],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

export function textureParams(state: TextureFormState, units: Units): TextureParams | null {
  if (Object.keys(validateTexture(state, units)).length > 0) {
    return null;
  }
  const length = (text: string) => lengthValue(text, units) ?? 0;
  const number = (text: string) => numberValue(text) ?? 0;
  const minLength = number(state.minLength);
  const minDepth = number(state.minDepth);
  return {
    angleDeg: number(state.angle),
    angleVariationDeg: state.angleVariation,
    strokeLength: length(state.strokeLength),
    lengthVariation: 1 - minLength / 100,
    spacing: length(state.spacing),
    stepoverVariationPercent: number(state.stepoverVariation),
    startDepth: length(state.startDepth),
    depth: length(state.depth),
    depthVariation: 1 - minDepth / 100,
    seed: state.seed,
    minLengthPercent: minLength,
    minDepthPercent: minDepth,
    maxOverlapPercent: number(state.maxOverlap),
    overlapVariationPercent: number(state.overlapVariation),
    boundaryOffset: length(state.boundaryOffset),
    vectorsAsPattern: state.vectorsAsPattern,
    ramp: state.ramp,
  };
}

export function textureStateFrom(
  params: TextureParams,
  name: string,
  units: Units,
): TextureFormState {
  const base = initialTextureState(units);
  const minLength = params.minLengthPercent ?? null;
  const minDepth = params.minDepthPercent ?? null;
  const overlap = params.maxOverlapPercent ?? null;
  const variation = params.stepoverVariationPercent ?? null;
  return {
    name,
    startDepth: lengthText(params.startDepth ?? 0, units),
    depth: lengthText(params.depth, units),
    // Older toolpaths varied either way; their shortest is the least.
    minDepth: minDepth === null ? percentText(1 - params.depthVariation) : String(minDepth),
    strokeLength: lengthText(params.strokeLength, units),
    minLength: minLength === null ? percentText(1 - params.lengthVariation) : String(minLength),
    maxOverlap: overlap === null ? '0' : String(overlap),
    overlapVariation: String(params.overlapVariationPercent ?? 100),
    spacing: lengthText(params.spacing, units),
    // Older toolpaths moved strokes a tenth of the stepover either way.
    stepoverVariation: variation === null ? '10' : String(variation),
    angle: String(params.angleDeg),
    boundaryOffset: lengthText(params.boundaryOffset ?? 0, units),
    vectorsAsPattern: params.vectorsAsPattern ?? false,
    ramp: params.ramp ?? base.ramp,
    seed: params.seed,
    angleVariation: params.angleVariationDeg,
  };
}

/** `percent` of the length in `total`, as the text a slider shows beside it. */
export function partOf(total: string, percent: string, units: Units): string | null {
  const length = lengthValue(total, units);
  const part = numberValue(percent);
  if (length === null || part === null) {
    return null;
  }
  return lengthText((length * part) / 100, units);
}

/** The settings Save ... writes to a file. */
export function textureSettingsText(params: TextureParams): string {
  return `${JSON.stringify({ format: SETTINGS_FORMAT, version: 1, params }, null, 2)}\n`;
}

/** The settings in a file Save ... wrote, or null for any other file. */
export function textureSettingsFrom(text: string): TextureParams | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  if (typeof parsed !== 'object' || parsed === null) {
    return null;
  }
  const file = parsed as { format?: unknown; params?: unknown };
  if (file.format !== SETTINGS_FORMAT || typeof file.params !== 'object' || file.params === null) {
    return null;
  }
  const params = file.params as Partial<TextureParams>;
  const numbers: Array<keyof TextureParams> = [
    'angleDeg',
    'angleVariationDeg',
    'strokeLength',
    'lengthVariation',
    'spacing',
    'depth',
    'depthVariation',
    'seed',
  ];
  if (!numbers.every((key) => typeof params[key] === 'number')) {
    return null;
  }
  return params as TextureParams;
}

/** The form after Load ...: the file's settings under the form's own name. */
export function loadTextureSettings(
  state: TextureFormState,
  params: TextureParams,
  units: Units,
): TextureFormState {
  return textureStateFrom(params, state.name, units);
}
