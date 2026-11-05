// How the View menu toggles change what the 3D view draws: the job as shown
// (a rotary job laid flat when Auto-Wrapping View is off) and where the
// origin arrows and the modeling plane go.

import type { JobSetup } from '../../bridge/types';
import { isRotary } from '../../lib/rotary';
import { jobBounds } from '../View2D/camera';
import type { MarkerPlacement } from './sceneExtras';

/** The job as the view draws it: a rotary job lies flat unless it is wrapped. */
export function shownJob(job: JobSetup, autoWrap: boolean): JobSetup {
  return isRotary(job) && !autoWrap ? { ...job, kind: 'singleSided' } : job;
}

/** Where the origin arrows and the modeling plane sit for `job`, the plane
 * `depth` under the material top. */
export function markerPlacement(job: JobSetup, depth: number): MarkerPlacement {
  const bounds = jobBounds(job);
  const surface = job.zZero === 'bottom' ? job.thickness : 0;
  return {
    originY: -surface,
    planeY: -depth,
    bounds: [bounds.minX, bounds.minY, bounds.maxX, bounds.maxY],
  };
}
