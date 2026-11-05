// Scale Design in Job Setup: how the design stretches and moves when the job
// size changes. The old material rectangle maps onto the new one, so with no
// datum offset the design scales about the XY datum. Mirrors job_scale.rs.

import type { Matrix, Point } from '../bridge/document';
import type { JobSetup } from '../bridge/types';
import { jobBounds } from '../views/View2D/camera';

export interface DesignScale {
  /** New width over old width. */
  scaleX: number;
  /** New height over old height. */
  scaleY: number;
  /** The mapping as a matrix [a, b, c, d, e, f] in job millimetres. */
  matrix: Matrix;
}

const SAME = 1e-12;

/** How the design maps from `before` to `after`, or null when either job has no size. */
export function designScale(before: JobSetup, after: JobSetup): DesignScale | null {
  const from = jobBounds(before);
  const to = jobBounds(after);
  const oldWidth = from.maxX - from.minX;
  const oldHeight = from.maxY - from.minY;
  const newWidth = to.maxX - to.minX;
  const newHeight = to.maxY - to.minY;
  if (!(oldWidth > 0 && oldHeight > 0 && newWidth > 0 && newHeight > 0)) {
    return null;
  }
  const scaleX = newWidth / oldWidth;
  const scaleY = newHeight / oldHeight;
  return {
    scaleX,
    scaleY,
    matrix: [scaleX, 0, 0, scaleY, to.minX - from.minX * scaleX, to.minY - from.minY * scaleY],
  };
}

/** True when the mapping leaves every point where it is. */
export function isIdentityScale(scale: DesignScale): boolean {
  const [a, , , d, e, f] = scale.matrix;
  return (
    Math.abs(a - 1) < SAME && Math.abs(d - 1) < SAME && Math.abs(e) < SAME && Math.abs(f) < SAME
  );
}

/** Where the mapping sends `point`. */
export function applyScale(scale: DesignScale, point: Point): Point {
  const [a, b, c, d, e, f] = scale.matrix;
  return { x: a * point.x + c * point.y + e, y: b * point.x + d * point.y + f };
}
