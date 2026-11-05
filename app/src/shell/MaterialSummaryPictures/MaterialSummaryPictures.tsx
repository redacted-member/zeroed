import type { JSX } from 'react';

import type { JobSetup } from '../../bridge/types';
import { HelperPicture } from '../../forms/HelperPicture/HelperPicture';
import {
  rotaryZZeroPicture,
  xyDatumPicture,
  zZeroPicture,
} from '../../forms/HelperPicture/pictures/job';

/**
 * The Material Setup summary's two pictures: the block with its Z zero at
 * the left of the rows and the XY datum at their right. Loaded on demand
 * so the drawings stay out of the start bundle.
 */
export function MaterialSummaryPictures({
  job,
  side,
}: {
  job: JobSetup;
  side: 'left' | 'right';
}): JSX.Element | null {
  if (side === 'left') {
    return (
      <HelperPicture
        picture={
          job.kind === 'rotary'
            ? rotaryZZeroPicture(job.rotary.zZero ?? 'surface')
            : zZeroPicture(job.zZero)
        }
        size="small"
      />
    );
  }
  return job.kind === 'rotary' ? null : (
    <HelperPicture picture={xyDatumPicture(job.xyOrigin)} size="small" />
  );
}
