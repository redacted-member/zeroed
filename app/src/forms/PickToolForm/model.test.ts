import { describe, expect, it } from 'vitest';

import { bladeTo, rejoinIds, trimsHere } from './model';

describe('Interactive Vector Trim model', () => {
  it('trims on the press and again once the drag moves on', () => {
    expect(trimsHere(null, { x: 0, y: 0 }, 1)).toBe(true);
    expect(trimsHere({ x: 0, y: 0 }, { x: 0.5, y: 0 }, 1)).toBe(false);
    expect(trimsHere({ x: 0, y: 0 }, { x: 1.5, y: 0 }, 1)).toBe(true);
  });

  it('cuts along the drag from where the last cut ended', () => {
    expect(bladeTo(null, { x: 5, y: 5 }, 1)).toBeNull();
    expect(bladeTo({ x: 0, y: 0 }, { x: 0.5, y: 0 }, 1)).toBeNull();
    expect(bladeTo({ x: 0, y: -10 }, { x: 0, y: 10 }, 1)).toEqual({
      from: { x: 0, y: -10 },
      to: { x: 0, y: 10 },
    });
  });

  it('rejoins the trimmed vectors that are still there, once each', () => {
    expect(rejoinIds([3, 1, 3, 7], new Set([1, 3]))).toEqual([3, 1]);
    expect(rejoinIds([], new Set([1]))).toEqual([]);
  });
});
