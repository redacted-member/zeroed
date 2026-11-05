import { describe, expect, it } from 'vitest';

import type { Heightfield } from '../../bridge/cam';
import { floatHeights } from '../../bridge/heights';
import { heightfieldMesh } from './heightfieldMesh';
import { gridKey, ROLE, surfaceGrid, type GridShape } from './surfaceGrid';

const SHAPE: GridShape = {
  width: 4,
  height: 3,
  cellSize: 1,
  originX: 0.5,
  originY: 0.5,
  underside: false,
};

function flatField(shape: GridShape): Heightfield {
  return {
    ...shape,
    top: 0,
    bottom: -18,
    revision: 1,
    sections: [],
    z: floatHeights(new Float32Array(shape.width * shape.height)),
    owner: new Uint16Array(0),
  };
}

describe('GPU surface grid', () => {
  it('matches the CPU mesh layout: top surface, walls, then a flat base', () => {
    const grid = surfaceGrid(SHAPE);
    const cpu = heightfieldMesh(flatField(SHAPE));
    // Same vertices and triangles as the CPU mesh of a field with no holes.
    expect(grid.positions.length).toBe(cpu.positions.length);
    expect(grid.indices).toEqual(cpu.indices);
    // Top surface x and z match; the grid leaves y to the shader.
    for (let v = 0; v < 12; v += 1) {
      expect(grid.positions[v * 3]).toBeCloseTo(cpu.positions[v * 3] ?? NaN);
      expect(grid.positions[v * 3 + 2]).toBeCloseTo(cpu.positions[v * 3 + 2] ?? NaN);
      expect(grid.roles[v]).toBe(ROLE.top);
    }
    // Vertex 5 stands on cell (1, 1).
    expect([grid.cells[10], grid.cells[11]]).toEqual([1, 1]);
    // Walls pair an upper and a lower vertex; the base corners sit on the floor.
    expect(grid.roles[12]).toBe(ROLE.wallTop);
    expect(grid.roles[13]).toBe(ROLE.wallFloor);
    const count = grid.roles.length;
    expect(Array.from(grid.roles.slice(count - 4))).toEqual(Array(4).fill(ROLE.wallFloor));
  });

  it('gives walls horizontal normals and the base a vertical one', () => {
    const grid = surfaceGrid(SHAPE);
    const count = grid.roles.length;
    for (let v = 12; v < count - 4; v += 1) {
      expect(Math.abs(grid.normals[v * 3 + 1] ?? 1)).toBeLessThan(1e-6);
      const length = Math.hypot(grid.normals[v * 3] ?? 0, grid.normals[v * 3 + 2] ?? 0);
      expect(length).toBeCloseTo(1);
    }
    for (let v = count - 4; v < count; v += 1) {
      expect(Math.abs(grid.normals[v * 3 + 1] ?? 0)).toBeCloseTo(1);
    }
  });

  it('draws the underside as a second grid facing down', () => {
    const grid = surfaceGrid({ ...SHAPE, underside: true });
    const cpu = heightfieldMesh({
      ...flatField(SHAPE),
      floor: floatHeights(new Float32Array(12).fill(-18)),
    });
    expect(grid.indices).toEqual(cpu.indices);
    const count = grid.roles.length;
    expect(grid.roles[count - 1]).toBe(ROLE.under);
    expect(grid.normals[(count - 1) * 3 + 1]).toBe(-1);
  });

  it('samples large fields every few cells, ending on the last', () => {
    const grid = surfaceGrid({ ...SHAPE, width: 10, height: 5 }, 4);
    expect(grid.step).toBe(3);
    expect(grid.columns).toBe(4);
    expect(grid.rows).toBe(3);
    // The last column of the first row is cell 9.
    expect(grid.cells[3 * 2]).toBe(9);
  });

  it('keys the grid by what it depends on', () => {
    expect(gridKey(SHAPE)).toBe(gridKey({ ...SHAPE }));
    expect(gridKey(SHAPE)).not.toBe(gridKey({ ...SHAPE, underside: true }));
    expect(gridKey(SHAPE)).not.toBe(gridKey({ ...SHAPE, originX: 2 }));
  });
});
