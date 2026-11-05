// Where the modelling plane (the relief model's zero plane) sits in the
// material, as the kernel places it (JobSetup::modelling_plane_depth).

import type { JobSetup, Side } from '../bridge/types';

type PlaneJob = Pick<JobSetup, 'kind' | 'thickness' | 'machine'>;

/**
 * Depth of the modelling plane below the top side's material surface, in
 * millimetres, for a top side model `topPeak` high and, on a double sided
 * job, a bottom side model `bottomPeak` deep. Gap Above Model puts the top
 * peak the gap under the surface; Gap Below Model puts the lowest point of
 * the model the gap above the material bottom (a rotary job's axis).
 */
export function modellingPlaneDepth(job: PlaneJob, topPeak: number, bottomPeak: number): number {
  const gap = Math.max(job.machine.modelGap, 0);
  const depth =
    job.machine.gapBelow === true
      ? job.thickness - gap - (job.kind === 'doubleSided' ? Math.max(bottomPeak, 0) : 0)
      : gap + Math.max(topPeak, 0);
  return Math.max(Math.min(depth, job.thickness), 0);
}

/** The same depth measured under the face of `side`: on the bottom side of
 * a double sided job the plane lies `thickness - depth` under that face. */
export function sidePlaneDepth(
  job: PlaneJob,
  topPeak: number,
  bottomPeak: number,
  side: Side,
): number {
  const depth = modellingPlaneDepth(job, topPeak, bottomPeak);
  return job.kind === 'doubleSided' && side === 'bottom' ? job.thickness - depth : depth;
}
