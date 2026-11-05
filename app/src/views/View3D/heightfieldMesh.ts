// Turns a preview heightfield into triangle data for the 3D view: the cut
// top surface, four walls down to the material bottom, and the base. When
// the field carries the underside (a double sided preview) the base follows
// it, and cells cut through (NaN) are left open in both surfaces. Large
// heightfields are sampled every few cells so the mesh stays light enough
// to orbit smoothly. Scene axes follow the renderer: x -> x, job y -> -z,
// job z -> y with the uncut top at y zero.

import type { Heightfield } from '../../bridge/cam';
import { floorReader, heightReader } from '../../bridge/heights';

/** Most vertices along either side of the top surface. */
export const MAX_SIDE = 700;

export interface GridMesh {
  positions: Float32Array;
  indices: Uint32Array;
  /** Samples across and down the top surface. */
  columns: number;
  rows: number;
  /** Field columns and rows the top surface samples; its vertices come
   * first, row by row. */
  xs: number[];
  ys: number[];
}

/** Cell indices to sample along a side of `cells`, every `step`, ending on the last. */
export function samples(cells: number, step: number): number[] {
  const out: number[] = [];
  for (let i = 0; i < cells - 1; i += step) {
    out.push(i);
  }
  out.push(cells - 1);
  return out;
}

/** Cells between samples so neither side has more than `maxSide` of them. */
export function gridStep(width: number, height: number, maxSide: number): number {
  return Math.max(1, Math.ceil(Math.max(width, height) / maxSide));
}

export function heightfieldMesh(field: Heightfield, maxSide = MAX_SIDE): GridMesh {
  const { width, height, cellSize, originX, originY, top, bottom } = field;
  const step = gridStep(width, height, maxSide);
  const xs = samples(width, step);
  const ys = samples(height, step);
  const columns = xs.length;
  const rows = ys.length;
  const floor = bottom - top;

  const positions: number[] = [];
  const indices: number[] = [];
  const vertex = (x: number, y: number, z: number): number => {
    positions.push(x, y, z);
    return positions.length / 3 - 1;
  };
  const sceneX = (cx: number) => originX + cx * cellSize;
  const sceneZ = (cy: number) => -(originY + cy * cellSize);
  const z = heightReader(field.z);
  const underside = floorReader(field);
  const isHole = (cx: number, cy: number) => Number.isNaN(z(cy * width + cx));
  const heightOf = (cx: number, cy: number) => {
    const h = z(cy * width + cx);
    return Number.isNaN(h) ? floor : h - top;
  };
  const floorOf = (cx: number, cy: number) => {
    const under = underside === null ? Number.NaN : underside(cy * width + cx);
    return Number.isNaN(under) ? floor : under - top;
  };
  /** A sampled grid over the surface; quads touching a hole are left out. */
  const grid = (height: (cx: number, cy: number) => number, facingUp: boolean) => {
    const start = positions.length / 3;
    for (const cy of ys) {
      for (const cx of xs) {
        vertex(sceneX(cx), height(cx, cy), sceneZ(cy));
      }
    }
    for (let r = 0; r + 1 < rows; r += 1) {
      for (let c = 0; c + 1 < columns; c += 1) {
        const corners: Array<[number, number]> = [
          [c, r],
          [c + 1, r],
          [c, r + 1],
          [c + 1, r + 1],
        ];
        if (corners.some(([gc, gr]) => isHole(xs[gc] ?? 0, ys[gr] ?? 0))) {
          continue;
        }
        const a = start + r * columns + c;
        const b = a + 1;
        const d = a + columns;
        const e = d + 1;
        if (facingUp) {
          indices.push(a, b, d, b, e, d);
        } else {
          indices.push(a, d, b, b, d, e);
        }
      }
    }
  };

  // Top surface.
  grid(heightOf, true);

  // Walls: each edge of the surface joined straight down to the floor.
  const wall = (edge: Array<[number, number]>) => {
    const start = positions.length / 3;
    for (const [cx, cy] of edge) {
      vertex(sceneX(cx), heightOf(cx, cy), sceneZ(cy));
      vertex(sceneX(cx), floorOf(cx, cy), sceneZ(cy));
    }
    for (let k = 0; k + 1 < edge.length; k += 1) {
      const upper = start + 2 * k;
      indices.push(upper, upper + 1, upper + 2, upper + 1, upper + 3, upper + 2);
    }
  };
  const lastX = width - 1;
  const lastY = height - 1;
  wall(xs.map((cx) => [cx, 0]));
  wall(xs.map((cx) => [cx, lastY]));
  wall(ys.map((cy) => [0, cy]));
  wall(ys.map((cy) => [lastX, cy]));

  // Base: the underside when the other side cut into it, else flat.
  if (underside !== null) {
    grid(floorOf, false);
    return {
      positions: Float32Array.from(positions),
      indices: Uint32Array.from(indices),
      columns,
      rows,
      xs,
      ys,
    };
  }
  const base = [
    vertex(sceneX(0), floor, sceneZ(0)),
    vertex(sceneX(lastX), floor, sceneZ(0)),
    vertex(sceneX(lastX), floor, sceneZ(lastY)),
    vertex(sceneX(0), floor, sceneZ(lastY)),
  ];
  const [p0, p1, p2, p3] = base as [number, number, number, number];
  indices.push(p0, p2, p1, p0, p3, p2);

  return {
    positions: Float32Array.from(positions),
    indices: Uint32Array.from(indices),
    columns,
    rows,
    xs,
    ys,
  };
}
