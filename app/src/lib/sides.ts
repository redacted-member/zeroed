// Double sided jobs: which side is shown and how sides are labelled. The
// kernel owns each side's content and the mapping between the sides; these
// helpers only mirror its rules for the views and forms.

import type { DoubleSidedSetup, JobSetup, Side } from '../bridge/types';
import { strings } from './strings';

export const DEFAULT_DOUBLE_SIDED: DoubleSidedSetup = { flip: 'horizontal', bottomZZero: 'top' };

export const SIDES: readonly Side[] = ['top', 'bottom'];

export function isDoubleSided(job: JobSetup): boolean {
  return job.kind === 'doubleSided';
}

/** The job's double sided settings, with defaults for an older kernel. */
export function doubleSidedOf(job: JobSetup): DoubleSidedSetup {
  return job.doubleSided ?? DEFAULT_DOUBLE_SIDED;
}

export function sideLabel(side: Side): string {
  return side === 'top' ? strings.sides.top : strings.sides.bottom;
}

/** The side a toolpath list row names, or null when the job has one side. */
export function toolpathSideTag(side: Side | undefined, job: JobSetup): string | null {
  if (!isDoubleSided(job)) {
    return null;
  }
  return sideLabel(side ?? 'top');
}

/** The side a side switching command asks for, or null when it does not apply. */
export function sideRequest(job: JobSetup, current: Side, wanted: Side): Side | null {
  if (!isDoubleSided(job) || wanted === current) {
    return null;
  }
  return wanted;
}
