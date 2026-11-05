import { describe, expect, it } from 'vitest';

import type { Heightfield } from '../../bridge/cam';
import { floatHeights } from '../../bridge/heights';
import { DEFAULT_JOB, type JobSetup } from '../../bridge/types';
import { normaliseRotary } from '../../lib/rotary';
import { stockRadius, wrappedHeightfield, wrappedScenePoint, wrappedStock } from './wrappedMesh';

const job: JobSetup = normaliseRotary({
  ...DEFAULT_JOB,
  kind: 'rotary',
  rotary: { diameter: 20, axis: 'x', length: 100, blank: 'cylinder', zZero: 'centre' },
});
const box = { minX: 0, minY: 0, maxX: 100, maxY: Math.PI * 20 };

/** Distance of every surface vertex (not the cap hubs) from the X axis at y = -10. */
function radii(positions: Float32Array, count: number): number[] {
  const out: number[] = [];
  for (let v = 0; v < count; v += 1) {
    const y = (positions[v * 3 + 1] ?? 0) + 10;
    const z = positions[v * 3 + 2] ?? 0;
    out.push(Math.hypot(y, z));
  }
  return out;
}

describe('wrapped rotary mesh', () => {
  it('wraps the bare blank into a closed cylinder of the blank radius', () => {
    const mesh = wrappedStock(job, box);
    const surface = mesh.along * mesh.around;
    for (const r of radii(mesh.positions, surface)) {
      expect(r).toBeCloseTo(10, 4);
    }
    // Two hubs, quads all the way round and a fan at each end.
    expect(mesh.positions.length / 3).toBe(surface + 2);
    expect(mesh.indices.length).toBe(mesh.around * 6 + 2 * mesh.around * 3);
    const maxIndex = Math.max(...mesh.indices);
    expect(maxIndex).toBeLessThan(mesh.positions.length / 3);
  });

  it('carries job coordinates for textures and the depth for the lithophane', () => {
    const mesh = wrappedStock(job, box);
    const count = mesh.positions.length / 3;
    expect(mesh.job.length).toBe(count * 3);
    const surface = mesh.along * mesh.around;
    // Surface vertices are at full depth; the hubs are on the axis.
    for (let v = 0; v < surface; v += 1) {
      expect(mesh.job[v * 3 + 2]).toBeCloseTo(2, 5);
      expect(mesh.job[v * 3]).toBeGreaterThanOrEqual(-1e-6);
      expect(mesh.job[v * 3]).toBeLessThanOrEqual(1 + 1e-6);
    }
    expect(mesh.job[surface * 3 + 2]).toBeCloseTo(1, 5);
    // The far end of the blank is u = 1.
    expect(mesh.job[(surface - 1) * 3]).toBeCloseTo(1, 5);
  });

  it('makes a square blank reach its corners', () => {
    const square = {
      diameter: 20,
      axis: 'x' as const,
      length: 1,
      blank: 'square' as const,
      zZero: 'centre' as const,
    };
    expect(stockRadius(square, 0)).toBeCloseTo(10, 9);
    expect(stockRadius(square, Math.PI / 4)).toBeCloseTo(10 * Math.SQRT2, 9);
  });

  it('pulls cut cells in towards the axis', () => {
    const width = 3;
    const height = 4;
    const top = 10;
    const z = new Float32Array(width * height).fill(top);
    z[1 * width + 1] = top - 4;
    z[2 * width + 2] = Number.NaN;
    const field: Heightfield = {
      width,
      height,
      cellSize: 1,
      originX: 0,
      originY: 0,
      top,
      bottom: 0,
      revision: 1,
      sections: [],
      z: floatHeights(z),
      owner: new Uint16Array(width * height),
    };
    const mesh = wrappedHeightfield(job, field);
    expect(mesh.along).toBe(width);
    expect(mesh.around).toBe(height);
    const r = radii(mesh.positions, width * height);
    // Vertex (along i, around j) is at i * around + j.
    expect(r[0]).toBeCloseTo(10, 4);
    expect(r[1 * height + 1]).toBeCloseTo(6, 4);
    expect(r[2 * height + 2]).toBeCloseTo(0, 4);
  });

  it('says which field cells its vertices sample, for the check marks', () => {
    const width = 3;
    const height = 4;
    const field: Heightfield = {
      width,
      height,
      cellSize: 1,
      originX: 0,
      originY: 0,
      top: 10,
      bottom: 0,
      revision: 1,
      sections: [],
      z: floatHeights(new Float32Array(width * height).fill(10)),
      owner: new Uint16Array(width * height),
    };
    const mesh = wrappedHeightfield(job, field);
    // Turning about X: rows run along X, columns round the blank (Y).
    expect(mesh.grid).toEqual({ rows: [0, 1, 2], columns: [0, 1, 2, 3], rowsAreY: false });
    expect(mesh.along * mesh.around).toBe(12);
  });

  it('places a job point on the wrapped blank as deep as it lies below the top', () => {
    // At the top of the blank (around 0): straight up from the axis.
    const [x, y, z] = wrappedScenePoint(job, 10, { x: 40, y: 0, z: 7 });
    expect(x).toBeCloseTo(40, 6);
    expect(z).toBeCloseTo(0, 6);
    // 3 below the uncut surface: 3 in from the radius 10, the axis at y -10.
    expect(y + 10).toBeCloseTo(7, 6);
    // A quarter turn round, the point faces sideways.
    const side = wrappedScenePoint(job, 10, { x: 40, y: (Math.PI * 20) / 4, z: 10 });
    expect(side[1]).toBeCloseTo(-10, 6);
    expect(Math.abs(side[2])).toBeCloseTo(10, 6);
  });
});
