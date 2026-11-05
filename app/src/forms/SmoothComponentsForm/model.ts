// Pure state for the Smooth Components form: a smoothing amount that
// previews live on the selected component, and whether its outline is kept.

import type { SmoothComponentArgs } from '../../bridge/modelRefine';

/** Where the slider starts and returns to after baking, percent. */
export const DEFAULT_SMOOTHING = 50;

export interface SmoothComponentsState {
  /** 0 to 100. */
  amount: number;
  preserveTransparency: boolean;
}

export function initialSmoothComponentsState(): SmoothComponentsState {
  return { amount: DEFAULT_SMOOTHING, preserveTransparency: true };
}

/** The model.smoothComponent arguments, or null without a component. */
export function smoothArgs(
  state: SmoothComponentsState,
  id: number | null,
  replace: number | undefined,
): SmoothComponentArgs | null {
  if (id === null) {
    return null;
  }
  const amount = Math.min(100, Math.max(0, Math.round(state.amount)));
  return {
    id,
    amount,
    preserveTransparency: state.preserveTransparency,
    ...(replace === undefined ? {} : { replace }),
  };
}

/** The state after Bake Current Smoothing: the slider back at its start. */
export function afterBake(state: SmoothComponentsState): SmoothComponentsState {
  return { ...state, amount: DEFAULT_SMOOTHING };
}
