// Triangle data for a rotary job in the 3D view: the flat job (or a preview
// heightfield over it) wrapped around the blank's axis. Distance around the
// axis becomes an angle, height above the uncut surface becomes a change of
// radius, and each end of the blank is closed with a fan to the axis. Scene
// axes follow the renderer (x -> x, job y -> -z, job z -> y) with the top of
// the uncut blank at scene y zero.

import type { Heightfield } from '../../bridge/cam';
import { heightReader } from '../../bridge/heights';
import type { JobSetup } from '../../bridge/types';
import { circumference, rotaryOf, wrapPoint, type RotaryBlankSetup } from '../../lib/rotary';
import type { VertexGrid } from './checkMarks';
import type { JobBox } from './jobShader';

/** Most samples along or around the blank. */
export const MAX_WRAP_SIDE = 360;

/** Samples around a bare blank. */
const STOCK_AROUND = 96;

export interface WrappedMesh {
  positions: Float32Array;
  indices: Uint32Array;
  /** Per vertex job coordinates for the texture and lithophane shading
   * (jobShader.ts): u, v across the job and the fraction of the blank's
   * depth under the vertex, plus one. */
  job: Float32Array;
  /** Samples along the axis and around it. */
  along: number;
  around: number;
  /** For a preview heightfield: the field cells the vertex rows (along
   * the axis) and columns (around it) sample. */
  grid?: VertexGrid;
}

/**
 * Distance from the axis to the uncut surface at `theta` radians from the
 * top: the radius for a round blank, the square's edge for a square one.
 */
export function stockRadius(rotary: RotaryBlankSetup, theta: number): number {
  const half = rotary.diameter / 2;
  if (rotary.blank === 'cylinder') {
    return half;
  }
  return half / Math.max(Math.abs(Math.cos(theta)), Math.abs(Math.sin(theta)));
}

/** Evenly spaced values from `start` to `end`, `count` of them (at least two). */
function spaced(start: number, end: number, count: number): number[] {
  const n = Math.max(2, count);
  const out: number[] = [];
  for (let i = 0; i < n; i += 1) {
    out.push(start + ((end - start) * i) / (n - 1));
  }
  return out;
}

interface Surface {
  /** Positions along the axis, in job units. */
  alongs: number[];
  /** Positions around the blank (flat distance), in job units. */
  arounds: number[];
  /** Distance from the axis at a sample. */
  radial: (i: number, j: number) => number;
  /** Whether the last ring around meets the first. */
  closed: boolean;
  /** The flat job the texture is stretched over. */
  box: JobBox;
}

function build(rotary: RotaryBlankSetup, surface: Surface): WrappedMesh {
  const { alongs, arounds, radial, closed, box } = surface;
  const positions: number[] = [];
  const indices: number[] = [];
  const job: number[] = [];
  const radius = rotary.diameter / 2;
  const fraction = (value: number, min: number, max: number) =>
    max - min > 0 ? (value - min) / (max - min) : 0;
  const put = (along: number, around: number, r: number): number => {
    const p = wrapPoint(rotary.axis, rotary.diameter, along, around, r);
    positions.push(p.x, p.y, p.z);
    const [x, y] = rotary.axis === 'x' ? [along, around] : [around, along];
    const surfaceR = radius > 0 ? stockRadius(rotary, around / radius) : 0;
    const depth = surfaceR > 0 ? Math.min(Math.max(r / surfaceR, 0), 1) : 0;
    job.push(fraction(x, box.minX, box.maxX), fraction(y, box.minY, box.maxY), 1 + depth);
    return positions.length / 3 - 1;
  };
  const columns = arounds.length;
  const rows = alongs.length;
  for (let i = 0; i < rows; i += 1) {
    for (let j = 0; j < columns; j += 1) {
      put(alongs[i] ?? 0, arounds[j] ?? 0, radial(i, j));
    }
  }
  const at = (i: number, j: number) => i * columns + j;
  const lastColumn = closed ? columns : columns - 1;
  for (let i = 0; i + 1 < rows; i += 1) {
    for (let j = 0; j < lastColumn; j += 1) {
      const next = (j + 1) % columns;
      // Wound so the faces look outward, away from the axis: the distance
      // round runs the other way against the axis for a blank along X.
      if (rotary.axis === 'x') {
        indices.push(
          at(i, j),
          at(i + 1, j),
          at(i, next),
          at(i, next),
          at(i + 1, j),
          at(i + 1, next),
        );
      } else {
        indices.push(
          at(i, j),
          at(i, next),
          at(i + 1, j),
          at(i, next),
          at(i + 1, next),
          at(i + 1, j),
        );
      }
    }
  }
  // End caps: a fan from the axis to each end ring.
  for (const i of [0, rows - 1]) {
    const along = alongs[i] ?? 0;
    const hub = put(along, 0, 0);
    for (let j = 0; j < lastColumn; j += 1) {
      const next = (j + 1) % columns;
      if ((i === 0) === (rotary.axis === 'y')) {
        indices.push(hub, at(i, next), at(i, j));
      } else {
        indices.push(hub, at(i, j), at(i, next));
      }
    }
  }
  return {
    positions: Float32Array.from(positions),
    indices: Uint32Array.from(indices),
    job: Float32Array.from(job),
    along: rows,
    around: columns,
  };
}

/** Job span along and around the axis: [min, max] for each. */
function spans(job: JobSetup, minX: number, minY: number, maxX: number, maxY: number) {
  return rotaryOf(job).axis === 'x'
    ? { along: [minX, maxX] as const, around: [minY, maxY] as const }
    : { along: [minY, maxY] as const, around: [minX, maxX] as const };
}

/** The bare blank for a rotary job whose flat layout spans the given box. */
export function wrappedStock(
  job: JobSetup,
  box: { minX: number; minY: number; maxX: number; maxY: number },
): WrappedMesh {
  const rotary = rotaryOf(job);
  const { along, around } = spans(job, box.minX, box.minY, box.maxX, box.maxY);
  const start = around[0];
  const full = circumference(rotary.diameter);
  // One full turn from where the layout starts, with the last ring left out
  // because the seam closes onto the first.
  const arounds = spaced(start, start + full, STOCK_AROUND + 1).slice(0, STOCK_AROUND);
  const radius = rotary.diameter / 2;
  return build(rotary, {
    alongs: [along[0], along[1]],
    arounds,
    radial: (_i, j) => stockRadius(rotary, (arounds[j] ?? 0) / radius),
    closed: true,
    box,
  });
}

/** Cell indices to sample along a side of `cells`, every `step`, ending on the last. */
function samples(cells: number, step: number): number[] {
  const out: number[] = [];
  for (let i = 0; i < cells - 1; i += step) {
    out.push(i);
  }
  out.push(cells - 1);
  return out;
}

/**
 * A preview heightfield wrapped round the blank. Cell heights are job Z; the
 * field's `top` is the uncut surface, so a cell `d` below it sits `d` inside
 * the blank's surface. Cells cut through (NaN) fall to the axis.
 */
export function wrappedHeightfield(
  job: JobSetup,
  field: Heightfield,
  maxSide = MAX_WRAP_SIDE,
): WrappedMesh {
  const rotary = rotaryOf(job);
  const { width, height, cellSize, originX, originY, top } = field;
  const step = Math.max(1, Math.ceil(Math.max(width, height) / maxSide));
  const xs = samples(width, step);
  const ys = samples(height, step);
  const alongX = rotary.axis === 'x';
  const alongCells = alongX ? xs : ys;
  const aroundCells = alongX ? ys : xs;
  const alongOrigin = alongX ? originX : originY;
  const aroundOrigin = alongX ? originY : originX;
  const heightAt = heightReader(field.z);
  const alongs = alongCells.map((c) => alongOrigin + c * cellSize);
  const arounds = aroundCells.map((c) => aroundOrigin + c * cellSize);
  const radius = rotary.diameter / 2;
  const span = (arounds[arounds.length - 1] ?? 0) - (arounds[0] ?? 0);
  const closed = span + 2 * cellSize * step >= circumference(rotary.diameter) * 0.98;
  const grid: VertexGrid = { rows: alongCells, columns: aroundCells, rowsAreY: !alongX };
  const mesh = build(rotary, {
    alongs,
    arounds,
    radial: (i, j) => {
      const cx = alongX ? alongCells[i] : aroundCells[j];
      const cy = alongX ? aroundCells[j] : alongCells[i];
      const z = heightAt((cy ?? 0) * width + (cx ?? 0));
      const surface = stockRadius(rotary, (arounds[j] ?? 0) / radius);
      if (Number.isNaN(z)) {
        return 0;
      }
      return Math.max(0, surface + (z - top));
    },
    closed,
    box: {
      minX: originX,
      minY: originY,
      maxX: originX + width * cellSize,
      maxY: originY + height * cellSize,
    },
  });
  return { ...mesh, grid };
}

/**
 * The scene point of job point (`x`, `y`) at job height `z` on a rotary
 * job whose preview field has its uncut surface at `top`: as far inside
 * the blank's surface there as `z` lies below `top`.
 */
export function wrappedScenePoint(
  job: JobSetup,
  top: number,
  at: { x: number; y: number; z: number },
): [number, number, number] {
  const rotary = rotaryOf(job);
  const alongX = rotary.axis === 'x';
  const [along, around] = alongX ? [at.x, at.y] : [at.y, at.x];
  const radius = rotary.diameter / 2;
  const surface = stockRadius(rotary, radius > 0 ? around / radius : 0);
  const p = wrapPoint(
    rotary.axis,
    rotary.diameter,
    along,
    around,
    Math.max(0, surface + (at.z - top)),
  );
  return [p.x, p.y, p.z];
}
