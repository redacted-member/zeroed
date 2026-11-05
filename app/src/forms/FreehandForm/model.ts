// Pure state for Freehand Drawing: the smoothness setting and turning the
// collected pointer trail into the kernel request and the live preview.

import type { Path, Point } from '../../bridge/document';
import type { FreehandArgs } from '../../bridge/drawTools';
import { fieldError, numberValue } from '../../lib/formFields';
import { freehandTolerance, smoothingPasses, trailCloses, trailLength } from '../../lib/freehand';

export interface FreehandFormState {
  /** 0 (follow the stroke) to 10 (smoothest). */
  smoothness: string;
  /** 0 (no damping) to 10 (largest dead zone round the brush). */
  stabilisation: string;
  /** End the stroke at the cursor rather than at the stabilised brush. */
  endAtCursor: boolean;
}

export const INITIAL_FREEHAND: FreehandFormState = {
  smoothness: '5',
  stabilisation: '3',
  endAtCursor: false,
};

/** The stabilisation to use, 0 to 10. */
export function stabilisationValue(state: FreehandFormState): number {
  const value = numberValue(state.stabilisation);
  return value === null ? 0 : Math.min(10, Math.max(0, value));
}

export function smoothnessError(state: FreehandFormState): string | undefined {
  return fieldError(numberValue(state.smoothness), { min: 0, max: 10 });
}

/** The smoothness to use; the default while the field is not valid. */
export function smoothnessValue(state: FreehandFormState): number {
  const value = numberValue(state.smoothness);
  return value === null || smoothnessError(state) !== undefined ? 5 : value;
}

/** The kernel request for a finished stroke, or null when the stroke is too
 * short on screen to be meant as a vector. The stroke closes when it ends
 * near its start, judged at the stabilised brush or at `release`, where the
 * pointer let go (the brush lags behind it by up to the dead zone). */
export function freehandRequest(
  trail: readonly Point[],
  state: FreehandFormState,
  pxPerMm: number,
  closePx: number,
  close = false,
  release?: Point,
): FreehandArgs | null {
  if (trail.length < 2 || !(pxPerMm > 0) || trailLength(trail) * pxPerMm < 3) {
    return null;
  }
  const smoothness = smoothnessValue(state);
  return {
    points: [...trail],
    smoothing: smoothingPasses(smoothness),
    tolerance: freehandTolerance(smoothness, pxPerMm),
    closed:
      (close && trail.length >= 3) ||
      trailCloses(trail, pxPerMm, closePx) ||
      (release !== undefined && trailCloses([...trail, release], pxPerMm, closePx)),
  };
}

/** The stroke as drawn so far, for the dashed preview. */
export function trailPreview(trail: readonly Point[]): Path[] {
  const [start, ...rest] = trail;
  if (start === undefined || rest.length === 0) {
    return [];
  }
  return [
    {
      subpaths: [
        { start, closed: false, segments: rest.map((to) => ({ kind: 'line' as const, to })) },
      ],
    },
  ];
}
