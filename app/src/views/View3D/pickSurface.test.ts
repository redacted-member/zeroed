import { describe, expect, it } from 'vitest';

import type { Heightfield } from '../../bridge/cam';
import { floatHeights } from '../../bridge/heights';
import { pickSurface, surfaceAt } from './pickSurface';

/** A 100 by 100 mm field of 1 mm cells, top at 10, with a 5 mm dip in the middle. */
function field(): Heightfield {
  const width = 100;
  const height = 100;
  const z = new Float32Array(width * height).fill(10);
  for (let j = 40; j < 60; j += 1) {
    for (let i = 40; i < 60; i += 1) {
      z[j * width + i] = 5;
    }
  }
  return {
    width,
    height,
    cellSize: 1,
    originX: 0.5,
    originY: 0.5,
    top: 10,
    bottom: 0,
    revision: 1,
    sections: [],
    z: floatHeights(z),
    owner: new Uint16Array(0),
  };
}

describe('surface picking', () => {
  it('reads heights in scene y under scene points', () => {
    expect(surfaceAt(field(), 10.5, -10.5)).toBe(0);
    expect(surfaceAt(field(), 50.5, -50.5)).toBe(-5);
    expect(surfaceAt(field(), 150, -10)).toBeNull();
  });

  it('finds the top straight below', () => {
    const hit = pickSurface(field(), { x: 20, y: 100, z: -30 }, { x: 0, y: -1, z: 0 });
    expect(hit?.x).toBeCloseTo(20);
    expect(hit?.y).toBeCloseTo(0, 2);
    expect(hit?.z).toBeCloseTo(-30);
  });

  it('finds the floor of the dip', () => {
    const hit = pickSurface(field(), { x: 50, y: 100, z: -50 }, { x: 0, y: -1, z: 0 });
    expect(hit?.y).toBeCloseTo(-5, 2);
  });

  it('meets the near rim first on a slanted ray', () => {
    const d = Math.SQRT1_2;
    // Starts above x = 0 and heads down and to the right at 45 degrees.
    const hit = pickSurface(field(), { x: 0, y: 30, z: -50 }, { x: d, y: -d, z: 0 });
    expect(hit?.x).toBeCloseTo(30, 0);
    expect(hit?.y).toBeCloseTo(0, 1);
  });

  it('misses when the ray passes beside the field', () => {
    expect(pickSurface(field(), { x: 200, y: 50, z: 0 }, { x: 0, y: -1, z: 0 })).toBeNull();
  });
});
