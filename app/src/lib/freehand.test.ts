import { describe, expect, it } from 'vitest';

import {
  extendTrail,
  freehandTolerance,
  ringPoints,
  smoothingPasses,
  stabilise,
  stabiliserPx,
  trailCloses,
  trailLength,
} from './freehand';

describe('extendTrail', () => {
  it('skips points closer than the screen step', () => {
    let trail = extendTrail([], { x: 0, y: 0 }, 10);
    trail = extendTrail(trail, { x: 0.1, y: 0 }, 10);
    expect(trail).toHaveLength(1);
    trail = extendTrail(trail, { x: 0.3, y: 0 }, 10);
    expect(trail).toEqual([
      { x: 0, y: 0 },
      { x: 0.3, y: 0 },
    ]);
  });
});

describe('trailCloses', () => {
  const loop = [
    { x: 0, y: 0 },
    { x: 10, y: 0 },
    { x: 10, y: 10 },
    { x: 0, y: 10 },
    { x: 0.2, y: 0.3 },
  ];

  it('closes a loop that ends near its start', () => {
    expect(trailLength(loop)).toBeCloseTo(30 + Math.hypot(0.2, 9.7), 6);
    expect(trailCloses(loop, 10, 8)).toBe(true);
    expect(trailCloses(loop, 100, 8)).toBe(false);
  });

  it('leaves short strokes and open trails open', () => {
    expect(trailCloses(loop.slice(0, 3), 10, 8)).toBe(false);
    expect(
      trailCloses(
        [
          { x: 0, y: 0 },
          { x: 0.5, y: 0 },
          { x: 0.1, y: 0 },
        ],
        10,
        8,
      ),
    ).toBe(false);
  });
});

describe('smoothness', () => {
  it('maps to a screen sized tolerance and a pass count', () => {
    expect(freehandTolerance(0, 10)).toBeCloseTo(0.05);
    expect(freehandTolerance(10, 10)).toBeCloseTo(0.3);
    expect(freehandTolerance(4, 0)).toBe(0.1);
    expect(smoothingPasses(0)).toBe(0);
    expect(smoothingPasses(5)).toBe(10);
    expect(smoothingPasses(40)).toBe(20);
  });
});

describe('stabiliser', () => {
  it('holds the brush inside the dead zone and drags it behind outside', () => {
    const brush = { x: 0, y: 0 };
    expect(stabilise(brush, { x: 1, y: 0 }, 2)).toBe(brush);
    expect(stabilise(brush, { x: 5, y: 0 }, 2)).toEqual({ x: 3, y: 0 });
    expect(stabilise(brush, { x: 5, y: 0 }, 0)).toEqual({ x: 5, y: 0 });
    expect(stabiliserPx(20)).toBe(40);
    expect(ringPoints({ x: 0, y: 0 }, 1, 4)).toHaveLength(5);
  });
});
