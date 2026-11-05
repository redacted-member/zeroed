import { describe, expect, it } from 'vitest';

import type { Heightfield } from '../../bridge/cam';
import { floatHeights } from '../../bridge/heights';
import { contourHoleWalls } from './holeWalls';

function field(
  width: number,
  height: number,
  hole: (x: number, y: number) => boolean,
): Heightfield {
  const z = new Float32Array(width * height).fill(-2);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      if (hole(x, y)) {
        z[y * width + x] = Number.NaN;
      }
    }
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

function corners(walls: { positions: Float32Array }): Array<[number, number, number]> {
  const out: Array<[number, number, number]> = [];
  for (let i = 0; i < walls.positions.length; i += 3) {
    out.push([walls.positions[i] ?? 0, walls.positions[i + 1] ?? 0, walls.positions[i + 2] ?? 0]);
  }
  return out;
}

describe('walls along the half way contour of a GPU surface', () => {
  it('has none without holes', () => {
    expect(contourHoleWalls(field(6, 6, () => false)).indices.length).toBe(0);
  });

  it('runs half way between a kept sample and a cut one, from the top to the floor', () => {
    // Cells x >= 3 cut through on an unsampled grid: the contour sits at x 2.5.
    const walls = contourHoleWalls(field(6, 4, (x) => x >= 3));
    const points = corners(walls);
    expect(points.length).toBeGreaterThan(0);
    for (const [x, y] of points) {
      expect(x).toBeCloseTo(2.5, 12);
      expect([-10, -2]).toContain(y);
    }
    // One quad per row of triangles crossed: 3 rows, each crossed by two
    // triangles whose pieces share corners, so 3 quads of 6 indices... the
    // contour crosses the diagonal too, giving two pieces per row.
    expect(walls.indices.length / 6).toBe(6);
  });

  it('shares its corners between the pieces so the normals smooth along the curve', () => {
    const walls = contourHoleWalls(field(8, 8, (x, y) => (x - 3.5) ** 2 + (y - 3.5) ** 2 < 5));
    const used = new Set(walls.indices);
    // Every corner is used by two pieces: a closed ring of walls.
    const counts = new Map<number, number>();
    for (const i of walls.indices) {
      counts.set(i, (counts.get(i) ?? 0) + 1);
    }
    for (const i of used) {
      expect(counts.get(i)).toBe(3);
    }
  });

  it('puts the contour where the sampled blocks are half cut through', () => {
    // Sampled every 4th cell (12 cells, 3 a side): cells x >= 5 cut. The
    // block round sample x 4 (cells 2..5) is one quarter cut, round x 8
    // (cells 6..9) fully cut: the contour sits at 4 + 4 * (0.5 - 0.25) / 0.75.
    const walls = contourHoleWalls(
      field(12, 12, (x) => x >= 5),
      3,
    );
    for (const [x] of corners(walls)) {
      expect(x).toBeCloseTo(4 + 4 * (0.25 / 0.75), 6);
    }
  });
});
