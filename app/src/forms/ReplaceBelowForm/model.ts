// Pure state for the Replace Below form: the height under which the
// chosen components (or the active level's) are emptied or planed, and
// whether what is left drops by that height.

import type { ReplaceBelowArgs, ReplaceWith } from '../../bridge/modelRefine';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';

export interface ReplaceBelowState {
  /** Replace Value Below, in the job units. */
  height: string;
  /** Replace Value With. */
  with: ReplaceWith;
  /** Lower Component by Value. */
  lower: boolean;
}

export type ReplaceBelowErrors = Partial<Record<'height', string>>;

/** What the form works on: the selected components, else the visible
 * components of the active level. */
export type ReplaceTarget = { kind: 'components'; ids: number[] } | { kind: 'level'; id: number };

export function initialReplaceBelowState(units: Units): ReplaceBelowState {
  return { height: lengthText(0, units), with: 'transparent', lower: false };
}

export function convertReplaceBelowUnits(
  state: ReplaceBelowState,
  from: Units,
  to: Units,
): ReplaceBelowState {
  return convertLengthFields(state, ['height'], from, to);
}

export function validateReplaceBelow(state: ReplaceBelowState, units: Units): ReplaceBelowErrors {
  const error = fieldError(lengthValue(state.height, units));
  return error === undefined ? {} : { height: error };
}

/** The target: the picked components, else the active level, else none. */
export function replaceTarget(
  picked: readonly number[],
  level: number | null,
): ReplaceTarget | null {
  if (picked.length > 0) {
    return { kind: 'components', ids: [...picked] };
  }
  return level === null ? null : { kind: 'level', id: level };
}

/** The kernel arguments, or null when the height is not a number or there
 * is nothing to work on. */
export function replaceBelowArgs(
  state: ReplaceBelowState,
  units: Units,
  target: ReplaceTarget | null,
): ReplaceBelowArgs | null {
  const height = lengthValue(state.height, units);
  if (
    target === null ||
    height === null ||
    validateReplaceBelow(state, units).height !== undefined
  ) {
    return null;
  }
  const common = { height, with: state.with, lower: state.lower };
  return target.kind === 'components'
    ? { ids: target.ids, ...common }
    : { level: target.id, ...common };
}

/** Whether the height is at or above the top, so nothing would be left.
 * Unknown heights never warn. */
export function removesEverything(
  state: ReplaceBelowState,
  units: Units,
  maxHeight: number | null,
): boolean {
  const height = lengthValue(state.height, units);
  return (
    state.with === 'transparent' && height !== null && maxHeight !== null && height >= maxHeight
  );
}
