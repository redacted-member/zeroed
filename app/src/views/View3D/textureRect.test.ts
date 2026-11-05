import { describe, expect, it } from 'vitest';

import { clipRect } from './textureRect';

describe('changed texture rectangle', () => {
  it('keeps a rectangle inside the texture', () => {
    expect(clipRect({ x0: 2, y0: 1, width: 3, height: 2 }, 10, 5)).toEqual({
      x0: 2,
      y0: 1,
      width: 3,
      height: 2,
    });
  });

  it('clips the rectangle to the texture', () => {
    expect(clipRect({ x0: -2, y0: 4, width: 20, height: 3 }, 10, 5)).toEqual({
      x0: 0,
      y0: 4,
      width: 10,
      height: 1,
    });
  });

  it('is null when nothing of the rectangle is inside', () => {
    expect(clipRect({ x0: 12, y0: 0, width: 3, height: 3 }, 10, 5)).toBeNull();
    expect(clipRect({ x0: 0, y0: 0, width: 0, height: 3 }, 10, 5)).toBeNull();
  });
});
