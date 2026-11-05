// How deep the 3D view's shadow shading is: 0.5 is the standard depth, 0
// close to normal shading, 1 the deepest. Ctrl + NumPad+ or - moves it one
// step for the session.

export const SHADOW_DEPTH = { initial: 0.5, min: 0.1, max: 1, step: 0.1 } as const;

/** The depth one step deeper (`by` 1) or lighter (-1), kept in range. */
export function stepShadowDepth(depth: number, by: 1 | -1): number {
  const next = Math.round((depth + by * SHADOW_DEPTH.step) * 10) / 10;
  return Math.min(SHADOW_DEPTH.max, Math.max(SHADOW_DEPTH.min, next));
}
