import { describe, expect, it } from 'vitest';

import { blockRule, blockSample } from './blockSampling';

describe('sampled blocks of the heightfield', () => {
  it('reads one cell when the grid is not sampled down', () => {
    expect(blockRule(1)).toEqual({ lo: 0, hi: 0, stride: 1 });
  });

  it('centres an even block half a cell low and reads every cell up to eight a side', () => {
    expect(blockRule(6)).toEqual({ lo: -3, hi: 2, stride: 1 });
    expect(blockRule(7)).toEqual({ lo: -3, hi: 3, stride: 1 });
    // 16 cells a side read every second cell: 8 samples.
    expect(blockRule(16)).toEqual({ lo: -8, hi: 7, stride: 2 });
  });

  it('averages the kept cells and counts the cut through share', () => {
    // A 4 by 4 block: the left half 10 mm, the right half 2 mm, one cell cut through.
    const z = (x: number, y: number) => (x === 5 && y === 4 ? Number.NaN : x < 4 ? 10 : 2);
    const rule = blockRule(4);
    const sample = blockSample(z, 20, 20, 4, 4, rule);
    // Cells x 2..5, y 2..5: 8 at 10 mm, 7 at 2 mm, one hole.
    expect(sample.hole).toBeCloseTo(1 / 16, 12);
    expect(sample.mean).toBeCloseTo((8 * 10 + 7 * 2) / 15, 12);
  });

  it('is all hole with NaN mean when every cell read is cut through', () => {
    const sample = blockSample(() => Number.NaN, 8, 8, 2, 2, blockRule(4));
    expect(sample.hole).toBe(1);
    expect(Number.isNaN(sample.mean)).toBe(true);
  });

  it('repeats the edge cells past the grid as the shader clamps', () => {
    const z = (x: number, y: number) => x + 10 * y;
    const sample = blockSample(z, 3, 3, 0, 0, blockRule(4));
    // Offsets -2..1 clamp to 0, 0, 0, 1: mean x is 0.25, mean y is 2.5.
    expect(sample.mean).toBeCloseTo(0.25 + 2.5, 12);
    expect(sample.hole).toBe(0);
  });
});
