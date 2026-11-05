import { describe, expect, it } from 'vitest';

import { distanceError, initialOffsetState, offsetArgs } from './model';

describe('offset form model', () => {
  it('builds outward radiused offsets by default', () => {
    expect(offsetArgs(initialOffsetState('mm'), 'mm')).toEqual({
      distance: 5,
      side: 'outward',
      corner: 'round',
      deleteOriginal: false,
    });
  });

  it('converts an inch distance and keeps the options', () => {
    const state = { ...initialOffsetState('inch'), distance: '0.5', side: 'inward' as const };
    expect(offsetArgs(state, 'inch')).toMatchObject({ distance: 12.7, side: 'inward' });
  });

  it('refuses a zero distance', () => {
    const state = { ...initialOffsetState('mm'), distance: '0' };
    expect(distanceError(state, 'mm')).toBeDefined();
    expect(offsetArgs(state, 'mm')).toBeNull();
  });
});
