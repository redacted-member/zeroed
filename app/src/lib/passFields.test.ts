import { describe, expect, it } from 'vitest';

import {
  initialPasses,
  passStateFrom,
  passValues,
  plannedDepths,
  steppedDepths,
} from './passFields';

const plain = { passCount: null, lastPassThickness: null, exactPassDepth: false, passList: null };

describe('pass planning (as the kernel plans it)', () => {
  it('stretches the pass depth by up to 15% to save a pass', () => {
    expect(steppedDepths(0, 11, 5, false)).toEqual([5.5, 11]);
    expect(steppedDepths(0, 12, 5, false)).toEqual([4, 8, 12]);
  });

  it('keeps the exact pass depth when asked', () => {
    expect(steppedDepths(0, 11, 5, true)).toEqual([5, 10, 11]);
    expect(steppedDepths(1, 11, 5, true)).toEqual([6, 11]);
  });

  it('refuses depths that make no sense', () => {
    expect(steppedDepths(5, 5, 1, false)).toBeNull();
    expect(steppedDepths(0, 5, 0, false)).toBeNull();
  });

  it('lets a custom list win, ending on the cut depth', () => {
    expect(
      plannedDepths(0, 10, 3, { ...plain, passCount: 9, passList: [7, 2, 2, 15, -1] }),
    ).toEqual([2, 7, 10]);
  });

  it('spreads a count and sets the last pass thickness apart', () => {
    expect(plannedDepths(0, 9, 3, { ...plain, passCount: 3 })).toEqual([3, 6, 9]);
    expect(plannedDepths(0, 10, 3, { ...plain, passCount: 3, lastPassThickness: 1 })).toEqual([
      4.5, 9, 10,
    ]);
  });
});

describe('pass values', () => {
  it('sends the custom list below the surface and reads it back from the start depth', () => {
    const state = { ...initialPasses('mm'), exact: true, list: [2, 4] };
    const values = passValues(state, 'mm', 3);
    expect(values).toEqual({
      passCount: null,
      lastPassThickness: null,
      exactPassDepth: true,
      passList: [5, 7],
    });
    const back = passStateFrom(null, null, 'mm', { exact: true, list: [5, 7], startDepth: 3 });
    expect(back.list).toEqual([2, 4]);
    expect(back.exact).toBe(true);
  });
});
