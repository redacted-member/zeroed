// Pure state for the Trim Objects form.

import type { TrimSide } from '../../bridge/document';

export interface TrimObjectsState {
  side: TrimSide;
  /** Fit arcs back onto the clipped shapes. */
  refit: boolean;
}

export function initialTrimObjectsState(): TrimObjectsState {
  return { side: 'outside', refit: true };
}

/**
 * Whether Apply can run: the boundary is the last selected vector, so there
 * must be something before it (a vector or a selected component).
 */
export function canTrim(vectorCount: number, componentCount: number): boolean {
  if (vectorCount === 0) {
    return false;
  }
  return vectorCount >= 2 || componentCount > 0;
}
