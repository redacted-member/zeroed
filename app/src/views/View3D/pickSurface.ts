// Finds where a view ray first meets the heightfield surface by marching
// along it in half cell steps and refining the crossing, which is far
// cheaper than ray casting the whole surface mesh on every pointer move.
// Scene axes follow the renderer: x -> x, job y -> -z, job z -> y with the
// uncut top at y zero.

import type { Heightfield } from '../../bridge/cam';
import { heightReader, type HeightRead } from '../../bridge/heights';

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

/** Most samples one pick may take, so a grazing ray cannot stall the view. */
const MAX_STEPS = 20000;
const REFINE_STEPS = 12;

/** Surface height in scene y under a scene point, or null outside the field.
 * `read` reads the field's heights, made once by a caller that asks often. */
export function surfaceAt(
  field: Heightfield,
  sceneX: number,
  sceneZ: number,
  read: HeightRead = heightReader(field.z),
): number | null {
  const cx = Math.round((sceneX - field.originX) / field.cellSize);
  const cy = Math.round((-sceneZ - field.originY) / field.cellSize);
  if (cx < 0 || cy < 0 || cx >= field.width || cy >= field.height) {
    return null;
  }
  const z = read(cy * field.width + cx);
  return Number.isNaN(z) ? null : z - field.top;
}

/** Parameter range along the ray that lies over the field's rectangle. */
function spanOverField(field: Heightfield, origin: Vec3, dir: Vec3): [number, number] | null {
  const half = field.cellSize / 2;
  const minX = field.originX - half;
  const maxX = field.originX + (field.width - 0.5) * field.cellSize;
  // Scene z runs the other way from job y.
  const minZ = -(field.originY + (field.height - 0.5) * field.cellSize);
  const maxZ = -(field.originY - half);
  let t0 = 0;
  let t1 = Number.POSITIVE_INFINITY;
  const clip = (o: number, d: number, lo: number, hi: number): boolean => {
    if (Math.abs(d) < 1e-12) {
      return o >= lo && o <= hi;
    }
    let a = (lo - o) / d;
    let b = (hi - o) / d;
    if (a > b) {
      [a, b] = [b, a];
    }
    t0 = Math.max(t0, a);
    t1 = Math.min(t1, b);
    return t0 <= t1;
  };
  // Heights never rise above the uncut top (scene y zero) or fall below the
  // material bottom; a little margin keeps the ends of the span sampled.
  const floor = field.bottom - field.top;
  if (
    !clip(origin.x, dir.x, minX, maxX) ||
    !clip(origin.z, dir.z, minZ, maxZ) ||
    !clip(origin.y, dir.y, floor - 1, 1)
  ) {
    return null;
  }
  return Number.isFinite(t1) ? [t0, t1] : null;
}

/** The first point where the ray (unit `dir`) meets the surface, or null. */
export function pickSurface(field: Heightfield, origin: Vec3, dir: Vec3): Vec3 | null {
  const span = spanOverField(field, origin, dir);
  if (span === null) {
    return null;
  }
  const [start, end] = span;
  const step = Math.max(field.cellSize / 2, (end - start) / MAX_STEPS);
  const at = (t: number): Vec3 => ({
    x: origin.x + dir.x * t,
    y: origin.y + dir.y * t,
    z: origin.z + dir.z * t,
  });
  const read = heightReader(field.z);
  const below = (t: number): boolean | null => {
    const p = at(t);
    const surface = surfaceAt(field, p.x, p.z, read);
    return surface === null ? null : p.y <= surface;
  };
  let previous = start;
  for (let t = start; t <= end; t += step) {
    const hit = below(t);
    if (hit === true) {
      if (t === start) {
        return at(t);
      }
      let lo = previous;
      let hi = t;
      for (let k = 0; k < REFINE_STEPS; k += 1) {
        const mid = (lo + hi) / 2;
        if (below(mid) === true) {
          hi = mid;
        } else {
          lo = mid;
        }
      }
      return at(hi);
    }
    previous = t;
  }
  return null;
}
