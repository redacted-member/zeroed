// Pure state for the Profile style inlay forms: Male Straight and Male
// Stepped (cut round the outside) and Female Hole (cut round the inside).
// They hold the Profile form's state, with the side fixed, and a stepped
// insert's step.

import type { ProfileParams, StraightInlayParams } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import {
  convertProfileUnits,
  initialProfileState,
  profileParams,
  profileStateFrom,
  validateProfile,
  type ProfileErrors,
  type ProfileFormState,
} from '../ProfileToolpathForm/model';
import { kindParams, type InlayKind } from '../StraightInlayToolpathForm/model';

const text = strings.toolpaths.straightInlay;

export interface InlayProfileState {
  profile: ProfileFormState;
  stepDepth: string;
  stepWidth: string;
}

export type InlayProfileErrors = ProfileErrors & Partial<Record<'stepDepth' | 'stepWidth', string>>;

/** The side each kind is cut on: an insert outside, a hole inside. */
function sideOf(kind: InlayKind): ProfileFormState['side'] {
  return kind === 'femaleHole' ? 'inside' : 'outside';
}

export function initialInlayProfileState(units: Units, kind: InlayKind): InlayProfileState {
  const inch = units === 'inch';
  return {
    profile: { ...initialProfileState(units), side: sideOf(kind), advanced: true },
    stepDepth: lengthText(inch ? 3.175 : 3, units),
    stepWidth: lengthText(inch ? 6.35 : 5, units),
  };
}

export function convertInlayProfileUnits(
  state: InlayProfileState,
  from: Units,
  to: Units,
): InlayProfileState {
  return {
    ...convertLengthFields(state, ['stepDepth', 'stepWidth'], from, to),
    profile: convertProfileUnits(state.profile, from, to),
  };
}

export function validateInlayProfile(
  state: InlayProfileState,
  kind: InlayKind,
  units: Units,
): InlayProfileErrors {
  const errors: InlayProfileErrors = validateProfile(state.profile, units);
  if (kind === 'maleStepped') {
    const width = fieldError(lengthValue(state.stepWidth, units), { positive: true });
    if (width !== undefined) {
      errors.stepWidth = width;
    }
    const step = lengthValue(state.stepDepth, units);
    const depth = fieldError(step, { positive: true });
    const cut = lengthValue(state.profile.depths.cutDepth, units);
    if (depth !== undefined) {
      errors.stepDepth = depth;
    } else if (step !== null && cut !== null && step >= cut) {
      errors.stepDepth = text.stepTooDeep;
    }
  }
  return errors;
}

/** Kernel params, or null while a field is invalid. */
export function inlayProfileParams(
  state: InlayProfileState,
  kind: InlayKind,
  units: Units,
): StraightInlayParams | null {
  if (Object.keys(validateInlayProfile(state, kind, units)).length > 0) {
    return null;
  }
  const options = profileParams({ ...state.profile, side: sideOf(kind) }, units);
  if (options === null) {
    return null;
  }
  return {
    ...kindParams(kind),
    startDepth: options.startDepth,
    cutDepth: options.cutDepth,
    passDepth: options.passDepth,
    // The inlay's Allowance Offset cuts this much more off.
    allowance: lengthValue(state.profile.allowance, units) ?? 0,
    pocketAllowance: 0,
    stepDepth: lengthValue(state.stepDepth, units) ?? 3,
    stepWidth: lengthValue(state.stepWidth, units) ?? 5,
    direction: options.direction,
    tabs: options.tabs,
    stepover: null,
    strategy: 'offset',
    rasterAngle: 0,
    clearanceTool: null,
    // Placed tabs travel with the Profile options, anchored to the vectors.
    profileOptions: options,
  };
}

/** Form state for an existing inlay toolpath; older ones have no Profile options. */
export function inlayProfileStateFrom(
  params: StraightInlayParams,
  name: string,
  kind: InlayKind,
  units: Units,
): InlayProfileState {
  const base = initialInlayProfileState(units, kind);
  const options: ProfileParams =
    params.profileOptions ??
    ({
      ...(profileParams(base.profile, units) ?? ({} as ProfileParams)),
      startDepth: params.startDepth,
      cutDepth: params.cutDepth,
      passDepth: params.passDepth,
      direction: params.direction,
      tabs: params.tabs,
    } satisfies ProfileParams);
  const profile = profileStateFrom(
    { ...options, startDepth: params.startDepth, cutDepth: params.cutDepth, side: sideOf(kind) },
    name,
    units,
  );
  return {
    profile: { ...profile, allowance: lengthText(params.allowance, units) },
    stepDepth: lengthText(params.stepDepth, units),
    stepWidth: lengthText(params.stepWidth, units),
  };
}
