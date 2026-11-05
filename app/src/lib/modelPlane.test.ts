import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB } from '../bridge/types';
import { modellingPlaneDepth, sidePlaneDepth } from './modelPlane';

describe('modelling plane', () => {
  const job = { ...DEFAULT_JOB, thickness: 18, machine: { ...DEFAULT_JOB.machine, modelGap: 3 } };

  it('puts the model top the gap above under the surface', () => {
    expect(modellingPlaneDepth(job, 5, 0)).toBe(8);
    expect(modellingPlaneDepth(job, 40, 0)).toBe(18);
  });

  it('keeps the model the gap below above the material bottom', () => {
    const below = { ...job, machine: { ...job.machine, gapBelow: true } };
    expect(modellingPlaneDepth(below, 5, 0)).toBe(15);
    expect(modellingPlaneDepth(below, 9, 0)).toBe(15);
    const two = { ...below, kind: 'doubleSided' as const };
    expect(modellingPlaneDepth(two, 5, 4)).toBe(11);
    expect(sidePlaneDepth(two, 5, 4, 'bottom')).toBe(7);
  });
});
