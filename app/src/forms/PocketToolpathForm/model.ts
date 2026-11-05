// Pure state for the Pocket Toolpath form.

import type { CutDirection, PocketParams, Tool } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import {
  convertPasses,
  initialPasses,
  passStateFrom,
  passValues,
  validatePasses,
  type PassErrors,
  type PassState,
} from '../../lib/passFields';
import {
  cutDepthText,
  convertDepths,
  depthValues,
  initialDepths,
  validateDepths,
  type DepthErrors,
  type DepthState,
} from '../../lib/toolpathFields';

/** The raster's boundary pass: none, before the lines or after them. */
export type ProfilePassChoice = 'none' | 'first' | 'last';

export interface PocketFormState {
  name: string;
  depths: DepthState;
  passes: PassState;
  strategy: PocketParams['strategy'];
  rasterAngle: string;
  profilePass: ProfilePassChoice;
  selectionOrder: boolean;
  direction: CutDirection;
  allowance: string;
  ramp: boolean;
  rampDistance: string;
  /** The tools after the first in the Tools list, in cutting order. The
   * first is the form's own tool. */
  laterTools: Tool[];
  /** An older toolpath's larger clearance tool, which becomes the first tool
   * of the list once the form has its tool. */
  legacyClearance: Tool | null;
}

export type PocketField = 'rasterAngle' | 'allowance' | 'rampDistance';
export type PocketErrors = DepthErrors & PassErrors & Partial<Record<PocketField, string>>;

export function initialPocketState(units: Units): PocketFormState {
  const inch = units === 'inch';
  return {
    name: '',
    depths: initialDepths(units, inch ? 6.35 : 6),
    passes: initialPasses(units),
    strategy: 'offset',
    rasterAngle: '0',
    profilePass: 'last',
    selectionOrder: false,
    direction: 'climb',
    allowance: lengthText(0, units),
    ramp: false,
    rampDistance: lengthText(inch ? 25.4 : 20, units),
    laterTools: [],
    legacyClearance: null,
  };
}

export function convertPocketUnits(
  state: PocketFormState,
  from: Units,
  to: Units,
): PocketFormState {
  return {
    ...convertLengthFields(state, ['allowance', 'rampDistance'], from, to),
    depths: convertDepths(state.depths, from, to),
    passes: convertPasses(state.passes, from, to),
  };
}

export function validatePocket(state: PocketFormState, units: Units): PocketErrors {
  const errors: PocketErrors = {
    ...validateDepths(state.depths, units),
    ...validatePasses(state.passes, units),
  };
  const checks: Array<[PocketField, string | undefined]> = [
    ['allowance', fieldError(lengthValue(state.allowance, units))],
  ];
  if (state.strategy === 'raster') {
    checks.push(['rasterAngle', fieldError(numberValue(state.rasterAngle), { min: 0, max: 90 })]);
  }
  if (state.ramp) {
    checks.push([
      'rampDistance',
      fieldError(lengthValue(state.rampDistance, units), { positive: true }),
    ]);
  }
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/**
 * The Tools list after an older two tool toolpath is loaded: its clearance
 * tool cut first, so it heads the list, followed by its main tool.
 */
export function withLegacyClearance(
  state: PocketFormState,
  main: Tool,
): { first: Tool; state: PocketFormState } {
  if (state.legacyClearance === null) {
    return { first: main, state };
  }
  return {
    first: state.legacyClearance,
    state: { ...state, laterTools: [main, ...state.laterTools], legacyClearance: null },
  };
}

/** Kernel parameters, or null while a field is invalid. */
export function pocketParams(state: PocketFormState, units: Units): PocketParams | null {
  const depths = depthValues(state.depths, units);
  if (depths === null || Object.keys(validatePocket(state, units)).length > 0) {
    return null;
  }
  return {
    ...depths,
    ...passValues(state.passes, units, depths.startDepth),
    stepover: null,
    strategy: state.strategy,
    rasterAngle: numberValue(state.rasterAngle) ?? 0,
    direction: state.direction,
    allowance: lengthValue(state.allowance, units) ?? 0,
    profilePass: state.profilePass !== 'none',
    profilePassWhen: state.profilePass === 'first' ? 'first' : 'last',
    selectionOrder: state.selectionOrder,
    ramp: { enabled: state.ramp, distance: lengthValue(state.rampDistance, units) ?? 20 },
    clearanceTool: null,
    restTools: state.laterTools,
  };
}

/** Form state for an existing toolpath's parameters. */
export function pocketStateFrom(params: PocketParams, name: string, units: Units): PocketFormState {
  return {
    name,
    depths: {
      startDepth: lengthText(params.startDepth, units),
      cutDepth: cutDepthText(params.startDepth, params.cutDepth, units),
    },
    passes: passStateFrom(params.passCount, params.lastPassThickness, units, {
      exact: params.exactPassDepth ?? false,
      list: params.passList ?? null,
      startDepth: params.startDepth,
    }),
    strategy: params.strategy,
    rasterAngle: String(params.rasterAngle),
    profilePass: params.profilePass ? (params.profilePassWhen ?? 'last') : 'none',
    selectionOrder: params.selectionOrder ?? false,
    direction: params.direction,
    allowance: lengthText(params.allowance, units),
    ramp: params.ramp.enabled,
    rampDistance: lengthText(params.ramp.distance, units),
    laterTools: params.restTools ?? [],
    legacyClearance: params.clearanceTool ?? null,
  };
}
