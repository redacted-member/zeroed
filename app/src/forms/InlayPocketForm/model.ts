// Pure state for the Female Inlay - Pocket form: the Pocket form's state,
// whose Pocket Allowance grows the cavity, cut to the inlay's depths with
// the Pocket form's tool list.

import type { PocketParams, StraightInlayParams } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { lengthText, lengthValue } from '../../lib/formFields';
import {
  initialPocketState,
  pocketParams,
  pocketStateFrom,
  type PocketFormState,
} from '../PocketToolpathForm/model';
import { kindParams } from '../StraightInlayToolpathForm/model';

/** Kernel params, or null while a field is invalid. */
export function inlayPocketParams(
  state: PocketFormState,
  units: Units,
): StraightInlayParams | null {
  const options = pocketParams(state, units);
  if (options === null) {
    return null;
  }
  const grow = lengthValue(state.allowance, units) ?? 0;
  return {
    ...kindParams('femalePocket'),
    startDepth: options.startDepth,
    cutDepth: options.cutDepth,
    passDepth: options.passDepth,
    allowance: 0,
    pocketAllowance: grow,
    stepDepth: 3,
    stepWidth: 5,
    direction: options.direction,
    tabs: { enabled: false, count: 0, length: 0, thickness: 0 },
    stepover: null,
    strategy: options.strategy,
    rasterAngle: options.rasterAngle,
    clearanceTool: null,
    // The allowance cuts round the edge: the cavity comes out larger.
    pocketOptions: { ...options, allowance: -grow, restTools: [] },
    restTools: options.restTools ?? [],
  };
}

/** Form state for an existing pocket inlay; older ones have no Pocket options. */
export function inlayPocketStateFrom(
  params: StraightInlayParams,
  name: string,
  units: Units,
): PocketFormState {
  const base = pocketParams(initialPocketState(units), units);
  if (base === null) {
    return initialPocketState(units);
  }
  const options: PocketParams = params.pocketOptions ?? {
    ...base,
    passDepth: params.passDepth,
    strategy: params.strategy,
    rasterAngle: params.rasterAngle,
    direction: params.direction,
    clearanceTool: params.clearanceTool,
  };
  const state = pocketStateFrom(
    {
      ...options,
      startDepth: params.startDepth,
      cutDepth: params.cutDepth,
      restTools: params.restTools ?? [],
    },
    name,
    units,
  );
  return { ...state, allowance: lengthText(params.pocketAllowance, units) };
}
