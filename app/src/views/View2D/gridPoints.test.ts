import { describe, expect, it } from 'vitest';

import { gridPointStep, gridValues } from './gridPoints';

describe('point grid', () => {
  it('uses the grid spacing until the points would crowd, then doubles it', () => {
    expect(gridPointStep(10, 2)).toBe(10);
    expect(gridPointStep(10, 0.5)).toBe(20);
    expect(gridPointStep(10, 0.1)).toBe(80);
    expect(gridPointStep(0, 1)).toBe(0);
  });

  it('lists the multiples of the step inside a range', () => {
    expect(gridValues(-5, 25, 10)).toEqual([0, 10, 20]);
    expect(gridValues(3, 4, 10)).toEqual([]);
    expect(gridValues(0, 100, 1, 5)).toHaveLength(5);
  });
});
