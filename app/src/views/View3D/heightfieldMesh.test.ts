import { describe, expect, it } from 'vitest';

import { floatHeights } from '../../bridge/heights';
import type { Heightfield } from '../../bridge/cam';
import { heightfieldMesh } from './heightfieldMesh';

function field(width: number, height: number, cut: (x: number, y: number) => number): Heightfield {
  const z = new Float32Array(width * height);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      z[y * width + x] = cut(x, y);
    }
  }
  return {
    width,
    height,
    cellSize: 1,
    originX: 0.5,
    originY: 0.5,
    top: 0,
    bottom: -18,
    revision: 1,
    sections: [],
    z: floatHeights(z),
    owner: new Uint16Array(width * height),
  };
}

describe('heightfield mesh', () => {
  it('keeps every cell of a small field and closes it with walls and a base', () => {
    const mesh = heightfieldMesh(field(4, 3, () => 0));
    expect(mesh.columns).toBe(4);
    expect(mesh.rows).toBe(3);
    const surface = 4 * 3;
    const walls = 2 * (4 + 4 + 3 + 3);
    expect(mesh.positions.length / 3).toBe(surface + walls + 4);
    const surfaceTriangles = 2 * 3 * 2;
    const wallTriangles = 2 * (3 + 3 + 2 + 2);
    expect(mesh.indices.length / 3).toBe(surfaceTriangles + wallTriangles + 2);
  });

  it('puts the uncut top at zero, cuts below it and the floor at minus the thickness', () => {
    const mesh = heightfieldMesh(field(3, 3, (x, y) => (x === 1 && y === 1 ? -4 : 0)));
    const heights = [];
    for (let i = 1; i < mesh.positions.length; i += 3) {
      heights.push(mesh.positions[i]);
    }
    expect(heights).toContain(0);
    expect(heights).toContain(-4);
    expect(Math.min(...heights.map((h) => h ?? 0))).toBe(-18);
    // Job y maps to minus scene z.
    expect(mesh.positions[2]).toBe(-0.5);
  });

  it('samples a large field down to the side limit, keeping the last row and column', () => {
    const mesh = heightfieldMesh(
      field(2000, 1000, () => 0),
      700,
    );
    expect(mesh.columns).toBeLessThanOrEqual(701);
    expect(mesh.rows).toBeLessThanOrEqual(701);
    const lastTopX = mesh.positions[(mesh.columns - 1) * 3];
    expect(lastTopX).toBe(0.5 + 1999);
  });

  it('leaves cells cut through open and follows the underside', () => {
    const through = field(3, 3, (x, y) => (x === 1 && y === 1 ? Number.NaN : 0));
    const floor = new Float32Array(9).fill(-18);
    floor[4] = Number.NaN;
    floor[0] = -12;
    const mesh = heightfieldMesh({ ...through, floor: floatHeights(floor) });
    // Two grids of nine vertices, walls, no flat base.
    expect(mesh.positions.length / 3).toBe(9 + 2 * (3 + 3 + 3 + 3) + 9);
    // Every quad touches the hole in the middle, so neither grid has faces.
    const wallTriangles = 2 * (2 + 2 + 2 + 2);
    expect(mesh.indices.length / 3).toBe(wallTriangles);
    expect(mesh.positions.every((value) => Number.isFinite(value))).toBe(true);
    const heights = [];
    for (let i = 1; i < mesh.positions.length; i += 3) {
      heights.push(mesh.positions[i]);
    }
    expect(heights).toContain(-12);
  });

  it('keeps faces away from a hole', () => {
    const big = field(4, 4, (x, y) => (x === 0 && y === 0 ? Number.NaN : 0));
    const mesh = heightfieldMesh({ ...big, floor: floatHeights(new Float32Array(16).fill(-18)) });
    const quadsPerGrid = 3 * 3 - 1;
    const wallTriangles = 2 * (3 + 3 + 3 + 3);
    expect(mesh.indices.length / 3).toBe(2 * 2 * quadsPerGrid + wallTriangles);
  });
});
