import { describe, expect, it } from 'vitest';

import { boundaryOffset, boundaryOffsetError, initialBoundaryState } from './model';

describe('boundary form model', () => {
  it('traces the whole selection with no offset by default', () => {
    const state = initialBoundaryState('mm');
    expect(state.rubberBand).toBe(false);
    expect(boundaryOffset(state, 'mm')).toBe(0);
  });

  it('offsets only with Offset Boundary on', () => {
    const off = { ...initialBoundaryState('mm'), offset: '3' };
    expect(boundaryOffset(off, 'mm')).toBe(0);
    expect(boundaryOffset({ ...off, useOffset: true }, 'mm')).toBe(3);
    expect(boundaryOffsetError({ ...off, offset: '-2' }, 'mm')).toBeUndefined();
  });

  it('refuses a negative offset', () => {
    const state = { ...initialBoundaryState('mm'), useOffset: true, offset: '-2' };
    expect(boundaryOffsetError(state, 'mm')).toBeDefined();
    expect(boundaryOffset(state, 'mm')).toBeNull();
  });
});
