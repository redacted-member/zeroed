// The crossing vectors checks of Preferences, Confirmations: before V-Groove,
// Pocket and Offset calculate, vectors that cross each other or themselves
// are reported according to the preference.

import type { IntersectionCheck } from '../bridge/prefs';
import { strings } from './strings';

/** What the caller does next: calculate, stop, or stop and open the
 * Vector Validator on the crossing vectors. */
export type CheckOutcome = 'go' | 'stop' | 'validator';

/** A place where the selected vectors cross (ops.crossings). */
export interface Crossing {
  ids: number[];
  x: number;
  y: number;
}

/** Decides what a calculation does about `crossings` under `setting`;
 * `ask` answers whether to calculate anyway (declining opens the
 * validator). */
export async function crossingOutcome(
  setting: IntersectionCheck,
  crossings: readonly Crossing[],
  ask: (count: number) => Promise<boolean> | boolean,
): Promise<CheckOutcome> {
  if (setting === 'skip' || setting === 'proceed' || crossings.length === 0) {
    return 'go';
  }
  if (setting === 'validator') {
    return 'validator';
  }
  return (await ask(crossings.length)) ? 'go' : 'validator';
}

/** Every vector id that takes part in a crossing, once each. */
export function crossingIds(crossings: readonly Crossing[]): number[] {
  return [...new Set(crossings.flatMap((c) => c.ids))];
}

/** What a crossing check is guarding: a toolpath calculation or an offset. */
export type CheckPurpose = 'toolpath' | 'offset';

/** The question asked before going on with `count` crossings. */
export function crossingQuestion(count: number, purpose: CheckPurpose): string {
  return purpose === 'offset'
    ? strings.toolpathShell.crossingsAskOffset(count)
    : strings.toolpathShell.crossingsAsk(count);
}
