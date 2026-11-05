// The point grid of the 2D view: a dot at every grid spacing, thinned by
// doubling the step while the dots would crowd closer than a few pixels.

/** Closest two grid points may be on screen, in pixels. */
export const MIN_POINT_GAP_PX = 6;

/** The step between drawn points: the grid spacing, doubled until far enough apart. */
export function gridPointStep(spacingMm: number, pxPerMm: number): number {
  if (!(spacingMm > 0) || !(pxPerMm > 0)) {
    return 0;
  }
  let step = spacingMm;
  while (step * pxPerMm < MIN_POINT_GAP_PX) {
    step *= 2;
  }
  return step;
}

/** Multiples of `step` from `min` to `max` inclusive, at most `limit` of them. */
export function gridValues(min: number, max: number, step: number, limit = 4000): number[] {
  if (!(step > 0) || max < min) {
    return [];
  }
  const out: number[] = [];
  for (let v = Math.ceil(min / step) * step; v <= max + 1e-9 && out.length < limit; v += step) {
    out.push(v === 0 ? 0 : v);
  }
  return out;
}
