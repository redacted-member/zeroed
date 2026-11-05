import { describe, expect, it } from 'vitest';

import type { Heightfield } from '../../bridge/cam';
import { floatHeights } from '../../bridge/heights';
import { holeWalls } from './holeWalls';

function field(width: number, height: number, holes: Array<[number, number]>): Heightfield {
  const z = new Float32Array(width * height).fill(-2);
  for (const [x, y] of holes) {
    z[y * width + x] = Number.NaN;
  }
  return {
    width,
    height,
    cellSize: 1,
    originX: 0,
    originY: 0,
    top: 0,
    bottom: -10,
    revision: 1,
    sections: [],
    z: floatHeights(z),
    owner: new Uint16Array(0),
  } as Heightfield;
}

describe('walls round through cuts', () => {
  it('has none without holes', () => {
    expect(holeWalls(field(4, 4, [])).indices.length).toBe(0);
  });

  it('rings a single hole cell with walls from the top to the floor', () => {
    const walls = holeWalls(field(5, 5, [[2, 2]]));
    // The quads round the hole are dropped: the ring of 3 by 3 samples
    // around it has 8 edges on its outside.
    expect(walls.indices.length / 6).toBe(8);
    const ys = new Set<number>();
    for (let i = 1; i < walls.positions.length; i += 3) {
      ys.add(walls.positions[i] ?? 0);
    }
    expect([...ys].sort((a, b) => a - b)).toEqual([-10, -2]);
  });

  it('adds nothing along the job edge, which has its own walls', () => {
    const walls = holeWalls(field(3, 3, [[0, 0]]));
    // Only the two inner edges of the dropped corner quad.
    expect(walls.indices.length / 6).toBe(2);
  });
});
