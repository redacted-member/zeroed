import { describe, expect, it } from 'vitest';

import { DEFAULT_SMOOTHING, afterBake, initialSmoothComponentsState, smoothArgs } from './model';

describe('smooth components form model', () => {
  it('starts at half strength keeping the outline', () => {
    expect(initialSmoothComponentsState()).toEqual({
      amount: DEFAULT_SMOOTHING,
      preserveTransparency: true,
    });
  });

  it('needs a component and clamps the amount', () => {
    const state = { amount: 140.4, preserveTransparency: false };
    expect(smoothArgs(state, null, undefined)).toBeNull();
    expect(smoothArgs(state, 3, undefined)).toEqual({
      id: 3,
      amount: 100,
      preserveTransparency: false,
    });
    expect(smoothArgs({ amount: -2, preserveTransparency: true }, 3, 9)).toEqual({
      id: 3,
      amount: 0,
      preserveTransparency: true,
      replace: 9,
    });
  });

  it('puts the slider back after baking', () => {
    expect(afterBake({ amount: 90, preserveTransparency: false })).toEqual({
      amount: DEFAULT_SMOOTHING,
      preserveTransparency: false,
    });
  });
});
