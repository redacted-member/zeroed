import { describe, expect, it } from 'vitest';

import { isDrag, measureBetween } from './measure';

describe('measureBetween', () => {
  it('gives distance, deltas and a counter clockwise angle', () => {
    const m = measureBetween({ x: 1, y: 1 }, { x: 4, y: 5 });
    expect(m.distance).toBeCloseTo(5);
    expect(m.dx).toBeCloseTo(3);
    expect(m.dy).toBeCloseTo(4);
    expect(m.angleDeg).toBeCloseTo(53.1301, 3);
  });

  it('keeps the angle between 0 and 360', () => {
    expect(measureBetween({ x: 0, y: 0 }, { x: -1, y: 0 }).angleDeg).toBeCloseTo(180);
    expect(measureBetween({ x: 0, y: 0 }, { x: 0, y: -2 }).angleDeg).toBeCloseTo(270);
    expect(measureBetween({ x: 0, y: 0 }, { x: 1, y: -1e-12 }).angleDeg).toBe(0);
    expect(measureBetween({ x: 2, y: 2 }, { x: 2, y: 2 })).toEqual({
      distance: 0,
      dx: 0,
      dy: 0,
      angleDeg: 0,
    });
  });
});

describe('isDrag', () => {
  it('compares screen travel with the threshold', () => {
    expect(isDrag({ x: 0, y: 0 }, { x: 0.3, y: 0 }, 10)).toBe(false);
    expect(isDrag({ x: 0, y: 0 }, { x: 0.4, y: 0 }, 10)).toBe(true);
    expect(isDrag({ x: 0, y: 0 }, { x: 9, y: 0 }, 0)).toBe(false);
  });
});
