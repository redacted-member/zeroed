import { describe, expect, it } from 'vitest';

import { canTrim, initialTrimObjectsState } from './model';

describe('Trim Objects model', () => {
  it('clears outside the boundary and refits by default', () => {
    expect(initialTrimObjectsState()).toEqual({ side: 'outside', refit: true });
  });

  it('needs a boundary and something to trim', () => {
    expect(canTrim(0, 0)).toBe(false);
    expect(canTrim(1, 0)).toBe(false);
    expect(canTrim(0, 2)).toBe(false);
    expect(canTrim(2, 0)).toBe(true);
    expect(canTrim(1, 1)).toBe(true);
  });
});
