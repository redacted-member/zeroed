// Walls round the cells cut through (NaN) in a heightfield: the surface
// meshes leave a hole open there, so without these the inside of a through
// cut shows nothing. Each wall runs along a sampled grid edge between the
// surface kept on one side and the hole on the other, from the top down to
// the underside. Same sampling and scene axes as heightfieldMesh.

import type { Heightfield } from '../../bridge/cam';
import { floorReader, heightReader } from '../../bridge/heights';
import { blockRule, blockSample } from './blockSampling';
import { gridStep, MAX_SIDE, samples } from './heightfieldMesh';
import { GPU_MAX_SIDE } from './surfaceGrid';

export interface WallMesh {
  positions: Float32Array;
  indices: Uint32Array;
  /** Smoothed normals, when the builder makes them; otherwise computed from the faces. */
  normals?: Float32Array;
}

/** The walls round `field`'s holes; empty when it has none. */
export function holeWalls(field: Heightfield, maxSide = MAX_SIDE): WallMesh {
  const { width, height, cellSize, originX, originY, top, bottom } = field;
  const step = gridStep(width, height, maxSide);
  const xs = samples(width, step);
  const ys = samples(height, step);
  const columns = xs.length;
  const rows = ys.length;
  const floor = bottom - top;
  const cell = (c: number, r: number) => (ys[r] ?? 0) * width + (xs[c] ?? 0);
  const z = heightReader(field.z);
  const underside = floorReader(field);
  const hole = new Uint8Array(columns * rows);
  let any = false;
  for (let r = 0; r < rows; r += 1) {
    for (let c = 0; c < columns; c += 1) {
      if (Number.isNaN(z(cell(c, r)))) {
        hole[r * columns + c] = 1;
        any = true;
      }
    }
  }
  const positions: number[] = [];
  const indices: number[] = [];
  if (!any) {
    return { positions: new Float32Array(0), indices: new Uint32Array(0) };
  }
  const isHole = (c: number, r: number) => hole[r * columns + c] === 1;
  /** A grid quad (lower left sample c, r) the surface leaves out. */
  const dropped = (c: number, r: number) =>
    isHole(c, r) || isHole(c + 1, r) || isHole(c, r + 1) || isHole(c + 1, r + 1);
  const exists = (c: number, r: number) => c >= 0 && r >= 0 && c + 1 < columns && r + 1 < rows;
  const topOf = (c: number, r: number) => z(cell(c, r)) - top;
  const floorOf = (c: number, r: number) => {
    const under = underside === null ? Number.NaN : underside(cell(c, r));
    return Number.isNaN(under) ? floor : under - top;
  };
  const sceneX = (c: number) => originX + (xs[c] ?? 0) * cellSize;
  const sceneZ = (r: number) => -(originY + (ys[r] ?? 0) * cellSize);
  const wall = (a: [number, number], b: [number, number]) => {
    const start = positions.length / 3;
    for (const [c, r] of [a, b]) {
      positions.push(sceneX(c), topOf(c, r), sceneZ(r));
      positions.push(sceneX(c), floorOf(c, r), sceneZ(r));
    }
    indices.push(start, start + 1, start + 2, start + 1, start + 3, start + 2);
  };
  /** Whether the edge between two kept samples borders a hole on exactly
   * one side: `p` and `q` are the quads on either side. */
  const borders = (p: [number, number], q: [number, number]) => {
    const pIn = exists(p[0], p[1]);
    const qIn = exists(q[0], q[1]);
    if (!pIn || !qIn) {
      return false;
    }
    return dropped(p[0], p[1]) !== dropped(q[0], q[1]);
  };
  for (let r = 0; r < rows; r += 1) {
    for (let c = 0; c < columns; c += 1) {
      if (isHole(c, r)) {
        continue;
      }
      // Edge along X to the next sample, between the quads below and above.
      if (c + 1 < columns && !isHole(c + 1, r) && borders([c, r - 1], [c, r])) {
        wall([c, r], [c + 1, r]);
      }
      // Edge along Y, between the quads to the left and right.
      if (r + 1 < rows && !isHole(c, r + 1) && borders([c - 1, r], [c, r])) {
        wall([c, r], [c, r + 1]);
      }
    }
  }
  return { positions: Float32Array.from(positions), indices: Uint32Array.from(indices) };
}

/**
 * Walls round the through cuts of a surface drawn on the GPU: the surface
 * drops every fragment whose hole share (interpolated between the sampled
 * blocks) passes a half, so its edge is the half way contour over the
 * sample grid's triangles. Each wall runs along one piece of that contour
 * from the surface height down to the underside, sharing its corners with
 * the next piece so the normals smooth along the curve.
 */
export function contourHoleWalls(field: Heightfield, maxSide = GPU_MAX_SIDE): WallMesh {
  const { width, height, cellSize, originX, originY, top, bottom } = field;
  const step = gridStep(width, height, maxSide);
  const xs = samples(width, step);
  const ys = samples(height, step);
  const columns = xs.length;
  const rows = ys.length;
  const floor = bottom - top;
  const rule = blockRule(step);
  const z = heightReader(field.z);
  const readZ = (x: number, y: number) => z(y * width + x);
  const underside = floorReader(field);
  const vertexCount = columns * rows;
  // The hole share per sample as the shader reads it, skipped (0) where
  // none of the block's corners or centre is cut through.
  const hole = new Float32Array(vertexCount);
  const mean = new Float32Array(vertexCount).fill(Number.NaN);
  let any = false;
  const probe = (cx: number, cy: number): number => {
    let n = 0;
    for (const [dx, dy] of [
      [0, 0],
      [rule.lo, rule.lo],
      [rule.hi, rule.lo],
      [rule.lo, rule.hi],
      [rule.hi, rule.hi],
    ]) {
      const x = Math.min(Math.max(cx + dx, 0), width - 1);
      const y = Math.min(Math.max(cy + dy, 0), height - 1);
      if (Number.isNaN(readZ(x, y))) {
        n += 1;
      }
    }
    return n;
  };
  for (let r = 0; r < rows; r += 1) {
    for (let c = 0; c < columns; c += 1) {
      const cx = xs[c] ?? 0;
      const cy = ys[r] ?? 0;
      const quick = probe(cx, cy);
      if (quick === 0) {
        continue;
      }
      any = true;
      const sample = blockSample(readZ, width, height, cx, cy, rule);
      hole[r * columns + c] = sample.hole;
      mean[r * columns + c] = sample.mean;
    }
  }
  if (!any) {
    return { positions: new Float32Array(0), indices: new Uint32Array(0) };
  }
  /** Height above the top of sample `v`, as the shader lifts it. */
  const heightOf = (v: number): number => {
    const cached = mean[v];
    if (!Number.isNaN(cached ?? Number.NaN)) {
      return (cached ?? 0) - top;
    }
    const c = v % columns;
    const r = (v - c) / columns;
    const cx = xs[c] ?? 0;
    const cy = ys[r] ?? 0;
    const own = blockSample(readZ, width, height, cx, cy, rule);
    if (!Number.isNaN(own.mean)) {
      mean[v] = own.mean;
      return own.mean - top;
    }
    // A block cut right through sits at the height of the blocks round it.
    let sum = 0;
    let n = 0;
    for (const [nx, ny] of [
      [cx - step, cy],
      [cx + step, cy],
      [cx, cy - step],
      [cx, cy + step],
    ]) {
      const near = blockSample(
        readZ,
        width,
        height,
        Math.min(Math.max(nx, 0), width - 1),
        Math.min(Math.max(ny, 0), height - 1),
        rule,
      );
      if (!Number.isNaN(near.mean)) {
        sum += near.mean;
        n += 1;
      }
    }
    const filled = n > 0 ? sum / n : bottom;
    mean[v] = filled;
    return filled - top;
  };
  const floorOf = (v: number): number => {
    const c = v % columns;
    const r = (v - c) / columns;
    const under = underside === null ? Number.NaN : underside((ys[r] ?? 0) * width + (xs[c] ?? 0));
    return Number.isNaN(under) ? floor : under - top;
  };
  const sceneX = (v: number) => originX + (xs[v % columns] ?? 0) * cellSize;
  const sceneZ = (v: number) => -(originY + (ys[(v - (v % columns)) / columns] ?? 0) * cellSize);
  const positions: number[] = [];
  const indices: number[] = [];
  /** The wall corner where the contour crosses the edge between samples p and q, made once. */
  const crossings = new Map<number, number>();
  const corner = (p: number, q: number): number => {
    const key = p < q ? p * vertexCount + q : q * vertexCount + p;
    const made = crossings.get(key);
    if (made !== undefined) {
      return made;
    }
    const fp = hole[p] ?? 0;
    const fq = hole[q] ?? 0;
    const t = fq === fp ? 0.5 : Math.min(Math.max((0.5 - fp) / (fq - fp), 0), 1);
    const x = sceneX(p) + (sceneX(q) - sceneX(p)) * t;
    const zz = sceneZ(p) + (sceneZ(q) - sceneZ(p)) * t;
    const y = heightOf(p) + (heightOf(q) - heightOf(p)) * t;
    const under = floorOf(p) + (floorOf(q) - floorOf(p)) * t;
    const index = positions.length / 3;
    positions.push(x, y, zz, x, under, zz);
    crossings.set(key, index);
    return index;
  };
  const cut = (v: number) => (hole[v] ?? 0) > 0.5;
  /** One wall piece where the contour crosses the triangle a, b, c (the surface's winding). */
  const triangle = (a: number, b: number, c: number) => {
    const ring = [a, b, c];
    let from = -1;
    let to = -1;
    for (let i = 0; i < 3; i += 1) {
      const p = ring[i] ?? 0;
      const q = ring[(i + 1) % 3] ?? 0;
      if (cut(p) === cut(q)) {
        continue;
      }
      if (cut(q)) {
        from = corner(p, q);
      } else {
        to = corner(p, q);
      }
    }
    if (from < 0 || to < 0) {
      return;
    }
    indices.push(from, from + 1, to, from + 1, to + 1, to);
  };
  for (let r = 0; r + 1 < rows; r += 1) {
    for (let c = 0; c + 1 < columns; c += 1) {
      const a = r * columns + c;
      const b = a + 1;
      const d = a + columns;
      const e = d + 1;
      triangle(a, b, d);
      triangle(b, e, d);
    }
  }
  return {
    positions: Float32Array.from(positions),
    indices: Uint32Array.from(indices),
    normals: contourNormals(positions, indices),
  };
}

/** Wall corners on either side of a corner, along the contour, whose pieces shape its normal. */
const SMOOTH_REACH = 4;

/**
 * Normals of the contour walls, each corner's averaged over the pieces up
 * to SMOOTH_REACH away along the contour, weighted by their length, so the
 * small turns of a contour traced over a grid shade as one smooth curve.
 * Each piece's own normal is horizontal and square to it.
 */
function contourNormals(positions: readonly number[], indices: readonly number[]): Float32Array {
  const corners = positions.length / 6;
  /** Per corner, the pieces it belongs to (as indices into `pieces`). */
  const at: number[][] = Array.from({ length: corners }, () => []);
  const pieces: Array<{ from: number; to: number; nx: number; nz: number; length: number }> = [];
  for (let i = 0; i < indices.length; i += 6) {
    const from = (indices[i] ?? 0) / 2;
    const to = (indices[i + 2] ?? 0) / 2;
    const dx = (positions[to * 6] ?? 0) - (positions[from * 6] ?? 0);
    const dz = (positions[to * 6 + 2] ?? 0) - (positions[from * 6 + 2] ?? 0);
    const length = Math.hypot(dx, dz);
    // Square to the piece in the plane of the surface, the hole on a fixed side.
    const piece = {
      from,
      to,
      nx: length > 0 ? dz / length : 0,
      nz: length > 0 ? -dx / length : 0,
      length,
    };
    at[from]?.push(pieces.length);
    at[to]?.push(pieces.length);
    pieces.push(piece);
  }
  const normals = new Float32Array(corners * 6);
  for (let c = 0; c < corners; c += 1) {
    let nx = 0;
    let nz = 0;
    const seen = new Set<number>();
    // Walk out both ways from the corner, piece by piece.
    let frontier = [c];
    for (let reach = 0; reach < SMOOTH_REACH && frontier.length > 0; reach += 1) {
      const next: number[] = [];
      for (const corner of frontier) {
        for (const index of at[corner] ?? []) {
          if (seen.has(index)) {
            continue;
          }
          seen.add(index);
          const piece = pieces[index];
          if (piece === undefined) {
            continue;
          }
          nx += piece.nx * piece.length;
          nz += piece.nz * piece.length;
          next.push(piece.from === corner ? piece.to : piece.from);
        }
      }
      frontier = next;
    }
    const length = Math.hypot(nx, nz) || 1;
    for (const v of [c * 2, c * 2 + 1]) {
      normals[v * 3] = nx / length;
      normals[v * 3 + 1] = 0;
      normals[v * 3 + 2] = nz / length;
    }
  }
  return normals;
}
