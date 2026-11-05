// Collecting a freehand pointer trail before the kernel smooths it and fits
// curves. Lengths are millimetres unless a name says pixels.

import type { Point } from '../bridge/document';

/** Screen pixels the pointer must travel before another trail point is kept. */
export const TRAIL_STEP_PX = 2;

/** Adds a point to the trail when it is far enough from the last one. */
export function extendTrail(trail: readonly Point[], p: Point, pxPerMm: number): Point[] {
  const last = trail[trail.length - 1];
  if (last === undefined) {
    return [p];
  }
  const step = pxPerMm > 0 ? TRAIL_STEP_PX / pxPerMm : 0;
  if (Math.hypot(p.x - last.x, p.y - last.y) < step) {
    return trail as Point[];
  }
  return [...trail, p];
}

/** Length of the trail. */
export function trailLength(trail: readonly Point[]): number {
  let total = 0;
  for (let i = 1; i < trail.length; i += 1) {
    const a = trail[i - 1];
    const b = trail[i];
    if (a !== undefined && b !== undefined) {
      total += Math.hypot(b.x - a.x, b.y - a.y);
    }
  }
  return total;
}

/** A sketch closes when it ends within `closePx` screen pixels of its start
 * and is long enough that the two ends are not simply the same short stroke. */
export function trailCloses(trail: readonly Point[], pxPerMm: number, closePx: number): boolean {
  const first = trail[0];
  const last = trail[trail.length - 1];
  if (first === undefined || last === undefined || trail.length < 3 || !(pxPerMm > 0)) {
    return false;
  }
  const gapPx = Math.hypot(last.x - first.x, last.y - first.y) * pxPerMm;
  return gapPx <= closePx && trailLength(trail) * pxPerMm > closePx * 4;
}

/** Fit tolerance in millimetres for a smoothness from 0 to 10 at a zoom: the
 * curve may stray further on screen the smoother it is asked to be. */
export function freehandTolerance(smoothness: number, pxPerMm: number): number {
  const px = 0.5 + Math.max(0, smoothness) * 0.25;
  return pxPerMm > 0 ? px / pxPerMm : 0.1;
}

/** Averaging passes the kernel runs for a smoothness from 0 to 10. */
export function smoothingPasses(smoothness: number): number {
  return Math.round(Math.min(10, Math.max(0, smoothness)) * 2);
}

/** The dead zone round the brush, in screen pixels, for a stabilisation from 0 to 10. */
export function stabiliserPx(stabilisation: number): number {
  return Math.min(10, Math.max(0, stabilisation)) * 4;
}

/**
 * The stabilised brush: it stays put while the cursor moves inside `radius`
 * of it, and is pulled along behind the cursor at that distance once the
 * cursor leaves, which damps out small jitters.
 */
export function stabilise(brush: Point, cursor: Point, radius: number): Point {
  const dx = cursor.x - brush.x;
  const dy = cursor.y - brush.y;
  const d = Math.hypot(dx, dy);
  if (d <= radius || d === 0) {
    return radius > 0 ? brush : cursor;
  }
  const t = (d - radius) / d;
  return { x: brush.x + dx * t, y: brush.y + dy * t };
}

/** A circle outline of `radius` round `centre`, as line spans, for the dead zone marker. */
export function ringPoints(centre: Point, radius: number, steps = 32): Point[] {
  const out: Point[] = [];
  for (let i = 0; i <= steps; i += 1) {
    const a = (i / steps) * Math.PI * 2;
    out.push({ x: centre.x + radius * Math.cos(a), y: centre.y + radius * Math.sin(a) });
  }
  return out;
}
