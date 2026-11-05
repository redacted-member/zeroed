// The Material Setup summary at the top of the Toolpaths panel.

import type { JobSetup } from '../../bridge/types';
import { lengthText } from '../../lib/formFields';
import { strings } from '../../lib/strings';
import { unitSuffix } from '../../lib/units';

export interface SummaryRow {
  label: string;
  value: string;
}

/**
 * The rows the summary shows for a job, in the job's units: the simple view,
 * or with `detailed` (Show detailed summary on toolpath tab) every setting.
 */
export function materialSummary(job: JobSetup, detailed = false): SummaryRow[] {
  const text = strings.rightPanel.material;
  const unit = unitSuffix(job.units);
  const length = (mm: number) => `${lengthText(mm, job.units)} ${unit}`;
  const rotary = job.kind === 'rotary';
  const zZero = rotary
    ? job.rotary.zZero === 'centre'
      ? text.cylinderCentre
      : text.cylinderSurface
    : job.zZero === 'bottom'
      ? text.machineBed
      : text.materialSurface;
  const size = rotary
    ? { label: text.diameter, value: length(job.rotary.diameter) }
    : { label: text.thickness, value: length(job.thickness) };
  const below = job.machine.gapBelow === true;
  const gapLabel = rotary
    ? below
      ? text.gapInside
      : text.gapOutside
    : below
      ? text.modelGapBelow
      : text.modelGap;
  const home = {
    label: text.home,
    value: text.homeValue(
      lengthText(job.machine.homeX, job.units),
      lengthText(job.machine.homeY, job.units),
      lengthText(job.machine.homeZ, job.units),
    ),
  };
  if (!detailed) {
    return [
      size,
      { label: text.zZero, value: zZero },
      { label: text.safeZ, value: length(job.machine.safeZ) },
      home,
    ];
  }
  return [
    {
      label: text.jobSize,
      value: text.sizeValue(
        `${lengthText(job.width, job.units)} ${unit}`,
        `${lengthText(job.height, job.units)} ${unit}`,
      ),
    },
    size,
    { label: text.zZero, value: zZero },
    { label: text.datum, value: text.datumNames[job.xyOrigin] },
    { label: gapLabel, value: length(job.machine.modelGap) },
    { label: text.clearance, value: length(job.machine.safeZ) },
    { label: text.plunge, value: length(job.machine.plungeGap) },
    home,
  ];
}
