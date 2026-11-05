// The fixed triangle grid the GPU surface is drawn with. It has the same
// layout as the CPU heightfield mesh (top surface, four walls, then the base
// or the underside) but no heights: every vertex carries the field cell it
// stands on and a role that tells the vertex shader which height texture
// lifts it. The grid depends only on the field's size and placement, so a
// preview that cuts more material reuses it and only the textures change.

import { gridStep, samples } from './heightfieldMesh';

/** Most vertices along either side of the GPU top surface. */
export const GPU_MAX_SIDE = 1024;

/** Where a vertex takes its height from, as the shader reads the role. */
export const ROLE = {
  /** A wall's upper edge: the top height, fixed wall normal. */
  wallTop: 1,
  /** The top surface: the top height, normal from the heights round it. */
  top: 2,
  /** A wall's lower edge or a corner of the flat base: the floor height. */
  wallFloor: 3,
  /** The underside of a double sided preview: floor heights and normals. */
  under: 4,
} as const;

/** What the grid depends on. */
export interface GridShape {
  width: number;
  height: number;
  cellSize: number;
  originX: number;
  originY: number;
  /** Draw the underside from a floor texture instead of a flat base. */
  underside: boolean;
}

export interface SurfaceGrid {
  /** Scene x and z; y is a placeholder the shader replaces. */
  positions: Float32Array;
  /** Wall and base normals; the surfaces get theirs in the shader. */
  normals: Float32Array;
  /** Field column and row per vertex. */
  cells: Float32Array;
  roles: Float32Array;
  indices: Uint32Array;
  /** Cells between samples. */
  step: number;
  columns: number;
  rows: number;
}

/** A key that changes whenever the grid for `shape` would. */
export function gridKey(shape: GridShape, maxSide = GPU_MAX_SIDE): string {
  const { width, height, cellSize, originX, originY, underside } = shape;
  return [width, height, cellSize, originX, originY, underside ? 1 : 0, maxSide].join(':');
}

export function surfaceGrid(shape: GridShape, maxSide = GPU_MAX_SIDE): SurfaceGrid {
  const { width, height, cellSize, originX, originY, underside } = shape;
  const step = gridStep(width, height, maxSide);
  const xs = samples(width, step);
  const ys = samples(height, step);
  const columns = xs.length;
  const rows = ys.length;
  const lastX = width - 1;
  const lastY = height - 1;
  const surfaceVertices = columns * rows;
  const wallVertices = 4 * (columns + rows);
  const vertexCount = surfaceVertices + wallVertices + (underside ? surfaceVertices : 4);
  const quads = (columns - 1) * (rows - 1);
  const wallQuads = 2 * (columns - 1) + 2 * (rows - 1);
  const indexCount = 6 * (quads + wallQuads + (underside ? quads : 1));

  const positions = new Float32Array(vertexCount * 3);
  const normals = new Float32Array(vertexCount * 3);
  const cells = new Float32Array(vertexCount * 2);
  const roles = new Float32Array(vertexCount);
  const indices = new Uint32Array(indexCount);
  let vertices = 0;
  let written = 0;

  /** Adds a vertex over cell (cx, cy); `y` only shapes the fixed normals. */
  const vertex = (cx: number, cy: number, y: number, role: number): number => {
    positions[vertices * 3] = originX + cx * cellSize;
    positions[vertices * 3 + 1] = y;
    positions[vertices * 3 + 2] = -(originY + cy * cellSize);
    cells[vertices * 2] = cx;
    cells[vertices * 2 + 1] = cy;
    roles[vertices] = role;
    vertices += 1;
    return vertices - 1;
  };
  const triangle = (a: number, b: number, c: number) => {
    indices[written] = a;
    indices[written + 1] = b;
    indices[written + 2] = c;
    written += 3;
  };
  const grid = (role: number, facingUp: boolean) => {
    const start = vertices;
    for (const cy of ys) {
      for (const cx of xs) {
        vertex(cx, cy, 0, role);
      }
    }
    for (let r = 0; r + 1 < rows; r += 1) {
      for (let c = 0; c + 1 < columns; c += 1) {
        const a = start + r * columns + c;
        const b = a + 1;
        const d = a + columns;
        const e = d + 1;
        if (facingUp) {
          triangle(a, b, d);
          triangle(b, e, d);
        } else {
          triangle(a, d, b);
          triangle(b, d, e);
        }
      }
    }
  };
  const fixedNormals = (from: number, fromIndex: number) => {
    for (let i = fromIndex; i < written; i += 3) {
      const a = indices[i] ?? 0;
      const b = indices[i + 1] ?? 0;
      const c = indices[i + 2] ?? 0;
      const ux = (positions[b * 3] ?? 0) - (positions[a * 3] ?? 0);
      const uy = (positions[b * 3 + 1] ?? 0) - (positions[a * 3 + 1] ?? 0);
      const uz = (positions[b * 3 + 2] ?? 0) - (positions[a * 3 + 2] ?? 0);
      const vx = (positions[c * 3] ?? 0) - (positions[a * 3] ?? 0);
      const vy = (positions[c * 3 + 1] ?? 0) - (positions[a * 3 + 1] ?? 0);
      const vz = (positions[c * 3 + 2] ?? 0) - (positions[a * 3 + 2] ?? 0);
      const nx = uy * vz - uz * vy;
      const ny = uz * vx - ux * vz;
      const nz = ux * vy - uy * vx;
      for (const v of [a, b, c]) {
        normals[v * 3] = (normals[v * 3] ?? 0) + nx;
        normals[v * 3 + 1] = (normals[v * 3 + 1] ?? 0) + ny;
        normals[v * 3 + 2] = (normals[v * 3 + 2] ?? 0) + nz;
      }
    }
    for (let v = from; v < vertices; v += 1) {
      const x = normals[v * 3] ?? 0;
      const y = normals[v * 3 + 1] ?? 0;
      const z = normals[v * 3 + 2] ?? 0;
      const length = Math.hypot(x, y, z) || 1;
      normals[v * 3] = x / length;
      normals[v * 3 + 1] = y / length;
      normals[v * 3 + 2] = z / length;
    }
  };
  const surfaceNormals = (from: number, y: number) => {
    for (let v = from; v < from + surfaceVertices; v += 1) {
      normals[v * 3 + 1] = y;
    }
  };

  // Top surface.
  grid(ROLE.top, true);
  surfaceNormals(0, 1);

  // Walls: each edge of the surface joined straight down to the floor.
  const wallsFrom = vertices;
  const wallsIndexFrom = written;
  const wall = (edge: Array<[number, number]>) => {
    const start = vertices;
    for (const [cx, cy] of edge) {
      vertex(cx, cy, 0, ROLE.wallTop);
      vertex(cx, cy, -1, ROLE.wallFloor);
    }
    for (let k = 0; k + 1 < edge.length; k += 1) {
      const upper = start + 2 * k;
      triangle(upper, upper + 1, upper + 2);
      triangle(upper + 1, upper + 3, upper + 2);
    }
  };
  wall(xs.map((cx) => [cx, 0]));
  wall(xs.map((cx) => [cx, lastY]));
  wall(ys.map((cy) => [0, cy]));
  wall(ys.map((cy) => [lastX, cy]));

  if (underside) {
    fixedNormals(wallsFrom, wallsIndexFrom);
    const underFrom = vertices;
    grid(ROLE.under, false);
    surfaceNormals(underFrom, -1);
  } else {
    const p0 = vertex(0, 0, -1, ROLE.wallFloor);
    const p1 = vertex(lastX, 0, -1, ROLE.wallFloor);
    const p2 = vertex(lastX, lastY, -1, ROLE.wallFloor);
    const p3 = vertex(0, lastY, -1, ROLE.wallFloor);
    triangle(p0, p2, p1);
    triangle(p0, p3, p2);
    fixedNormals(wallsFrom, wallsIndexFrom);
  }

  return { positions, normals, cells, roles, indices, step, columns, rows };
}
