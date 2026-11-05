// Plane measurements between two points, and telling a drag from a click,
// shared by the Measure tool and drag snipping. Lengths are millimetres.

import type { Point } from '../bridge/document';

export interface Measurement {
  distance: number;
  dx: number;
  dy: number;
  /** Direction from the first point to the second, 0 to 360 degrees
   * counter clockwise from +X. */
  angleDeg: number;
}

export function measureBetween(from: Point, to: Point): Measurement {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const distance = Math.hypot(dx, dy);
  let angleDeg = distance === 0 ? 0 : (Math.atan2(dy, dx) * 180) / Math.PI;
  if (angleDeg < 0) {
    angleDeg += 360;
  }
  // A full turn that rounds to 360 reads better as 0.
  if (angleDeg >= 360 - 1e-9) {
    angleDeg = 0;
  }
  return { distance, dx, dy, angleDeg };
}

/** Pointer travel, in screen pixels, below which a press and release is a click. */
export const DRAG_THRESHOLD_PX = 4;

/** Whether the pointer moved far enough between two job points to count as a drag. */
export function isDrag(from: Point, to: Point, pxPerMm: number): boolean {
  if (!(pxPerMm > 0)) {
    return false;
  }
  return Math.hypot(to.x - from.x, to.y - from.y) * pxPerMm >= DRAG_THRESHOLD_PX;
}
