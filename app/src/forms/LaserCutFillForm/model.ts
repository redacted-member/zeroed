// Pure state for the Laser Cut / Fill Toolpath form. Power fields are
// percentages of the machine maximum; an empty power or speed uses the
// laser tool's own setting.

import type { LaserCutFillParams, LaserHatch } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

/** The Strategy radio: a kerf compensated cut, or a hatch fill. */
export type LaserStrategy = 'outside' | 'inside' | 'on' | 'hatch';

export interface LaserCutFillState {
  name: string;
  strategy: LaserStrategy;
  /** Hatch Fill: burn round the vectors as well (Outline). */
  outline: boolean;
  /** Extra offset past the kerf for Cut Outside and Cut Inside. */
  allowance: string;
  power: string;
  speed: string;
  /** Cut Depth: the passes step down to it by the laser's pass depth. */
  cutDepth: string;
  /** No. Repeats. */
  repeats: string;
  /** Overscan, percent of each fill line. */
  overscan: string;
  lineInterval: string;
  angle: string;
  hatch: LaserHatch;
  /** Fill Regionally rather than Fill Together. */
  fillRegions: boolean;
}

export type LaserCutFillField =
  'repeats' | 'power' | 'speed' | 'cutDepth' | 'lineInterval' | 'angle' | 'overscan' | 'allowance';
export type LaserCutFillErrors = Partial<Record<LaserCutFillField, string>>;

const LENGTHS: Array<'speed' | 'lineInterval' | 'allowance' | 'cutDepth'> = [
  'speed',
  'lineInterval',
  'allowance',
  'cutDepth',
];

export function initialLaserCutFillState(units: Units): LaserCutFillState {
  return {
    name: '',
    strategy: 'outside',
    outline: true,
    allowance: lengthText(0, units),
    power: '',
    speed: '',
    cutDepth: lengthText(0, units),
    repeats: '1',
    overscan: '0',
    lineInterval: lengthText(units === 'inch' ? 0.127 : 0.1, units),
    angle: '0',
    hatch: 'lines',
    fillRegions: false,
  };
}

export function convertLaserCutFillUnits(
  state: LaserCutFillState,
  from: Units,
  to: Units,
): LaserCutFillState {
  return convertLengthFields(state, LENGTHS, from, to);
}

/** Whether the fill settings apply. */
export function fills(state: LaserCutFillState): boolean {
  return state.strategy === 'hatch';
}

/** Whether the Allowance applies (Cut Outside and Cut Inside only). */
export function takesAllowance(state: LaserCutFillState): boolean {
  return state.strategy === 'outside' || state.strategy === 'inside';
}

/** No. Passes: the Cut Depth over the laser's pass depth, rounded up; one
 * at the surface (as the kernel's laser_passes). */
export function laserPasses(cutDepth: number | null, passDepth: number): number {
  if (cutDepth === null || !(cutDepth > 1e-9) || !(passDepth > 0)) {
    return 1;
  }
  return Math.min(Math.max(Math.ceil(cutDepth / passDepth - 1e-9), 1), 100);
}

function optional(text: string, check: (value: number | null) => string | undefined) {
  return text.trim() === '' ? undefined : check(numberValue(text));
}

export function validateLaserCutFill(state: LaserCutFillState, units: Units): LaserCutFillErrors {
  const errors: LaserCutFillErrors = {};
  const percent = (value: number | null) => fieldError(value, { min: 0, max: 100 });
  const checks: Array<[LaserCutFillField, string | undefined]> = [
    ['repeats', fieldError(numberValue(state.repeats), { min: 1, max: 100, integer: true })],
    ['power', optional(state.power, percent)],
    [
      'speed',
      state.speed.trim() === ''
        ? undefined
        : fieldError(lengthValue(state.speed, units), { positive: true }),
    ],
    ['cutDepth', fieldError(lengthValue(state.cutDepth, units), { min: 0 })],
  ];
  if (takesAllowance(state)) {
    checks.push(['allowance', fieldError(lengthValue(state.allowance, units))]);
  }
  if (fills(state)) {
    checks.push(
      ['lineInterval', fieldError(lengthValue(state.lineInterval, units), { min: 0.01 })],
      ['angle', fieldError(numberValue(state.angle), { min: -360, max: 360 })],
      ['overscan', fieldError(numberValue(state.overscan), { min: 0, max: 100 })],
    );
  }
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

export function laserCutFillParams(
  state: LaserCutFillState,
  units: Units,
): LaserCutFillParams | null {
  if (Object.keys(validateLaserCutFill(state, units)).length > 0) {
    return null;
  }
  const opt = (text: string) => (text.trim() === '' ? null : numberValue(text));
  const hatch = state.strategy === 'hatch';
  return {
    mode: hatch ? (state.outline ? 'cutAndFill' : 'fill') : 'cut',
    side: state.strategy === 'hatch' ? 'on' : state.strategy,
    allowance: takesAllowance(state) ? (lengthValue(state.allowance, units) ?? 0) : 0,
    repeats: numberValue(state.repeats) ?? 1,
    cutDepth: lengthValue(state.cutDepth, units) ?? 0,
    power: opt(state.power),
    speed: state.speed.trim() === '' ? null : lengthValue(state.speed, units),
    fillPower: null,
    lineInterval: lengthValue(state.lineInterval, units) ?? 0.1,
    angle: numberValue(state.angle) ?? 0,
    hatch: state.hatch,
    bidirectional: true,
    overscan: 0,
    overscanPercent: hatch ? (numberValue(state.overscan) ?? 0) : 0,
    fillRegions: hatch && state.fillRegions,
  };
}

export function laserCutFillStateFrom(
  params: LaserCutFillParams,
  name: string,
  units: Units,
): LaserCutFillState {
  const opt = (value: number | null) => (value === null ? '' : String(value));
  return {
    name,
    strategy: params.mode === 'cut' ? params.side : 'hatch',
    outline: params.mode !== 'fill',
    allowance: lengthText(params.allowance ?? 0, units),
    power: opt(params.power),
    speed: params.speed === null ? '' : lengthText(params.speed, units),
    cutDepth: lengthText(params.cutDepth ?? 0, units),
    repeats: String(params.repeats ?? params.passes ?? 1),
    overscan: String(params.overscanPercent ?? 0),
    lineInterval: lengthText(params.lineInterval, units),
    angle: String(params.angle),
    hatch: params.hatch,
    fillRegions: params.fillRegions ?? false,
  };
}
