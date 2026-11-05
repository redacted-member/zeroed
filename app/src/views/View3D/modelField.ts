// The relief model as the 3D view shows it: sitting in the material on its
// modelling plane, with the highest point the Material Setup gap under the
// material top, so everything outside the components is down at that plane.
// A model taller than the material keeps its zero plane at the material
// bottom and stands out above the top, so its full shape still shows.
// Both sides of a double sided job stand on one shared modelling plane,
// facing away from each other; the gap measures the whole two sided model.

import type { Heightfield } from '../../bridge/cam';
import {
  codedHeights,
  fillNan,
  floatHeights,
  heightReader,
  shiftHeights,
  type Heights,
} from '../../bridge/heights';
import type { Composite } from '../../bridge/model';
import type { JobSetup, Side } from '../../bridge/types';
import { sidePlaneDepth } from '../../lib/modelPlane';

/** Depth of the modelling plane under the active side's material face, in
 * millimetres, for an active side model `activePeak` high and the other
 * side's `otherPeak` (double sided jobs only). */
export function planeDepth(
  activePeak: number,
  otherPeak: number,
  job: JobSetup,
  side: Side = 'top',
): number {
  const bottom = job.kind === 'doubleSided' && side === 'bottom';
  const topPeak = bottom ? otherPeak : activePeak;
  const bottomPeak = bottom ? activePeak : otherPeak;
  return sidePlaneDepth(job, topPeak, bottomPeak, side);
}

/** Model heights `h` as job Z `by + h`, NaN (no model) at `by`: coded
 * heights keep their codes under a moved base, so the model is never
 * copied out into floats. */
function lifted(z: Heights, by: number): Heights {
  const filled = fillNan(z, 0);
  if (filled.code !== null) {
    return shiftHeights(filled, by);
  }
  const out = new Float32Array(filled.data.length);
  for (let i = 0; i < out.length; i += 1) {
    const h = filled.data[i] ?? 0;
    out[i] = by + (Number.isNaN(h) ? 0 : h);
  }
  return floatHeights(out);
}

function surfaceOf(job: JobSetup): number {
  return job.zZero === 'bottom' ? job.thickness : 0;
}

/** The composite in the heightfield shape the mesh builder takes, in job Z,
 * its zero plane `depth` under the material top (by default where a single
 * sided job puts it). */
export function modelField(
  composite: Composite,
  job: JobSetup,
  depth = planeDepth(composite.max ?? 0, 0, job),
): Heightfield {
  const surface = surfaceOf(job);
  const zeroPlane = surface - depth;
  return {
    width: composite.width,
    height: composite.height,
    cellSize: composite.cellSize,
    originX: composite.originX,
    originY: composite.originY,
    top: surface,
    bottom: surface - job.thickness,
    revision: composite.revision,
    sections: [],
    z: lifted(composite.z, zeroPlane),
    owner: new Uint16Array(0),
  };
}

/** A flat composite on `other`'s grid, for a side with no model of its own. */
export function flatComposite(other: Composite): Composite {
  const zero = codedHeights(new Uint16Array(other.z.data.length), { base: 0, step: 0 });
  return { ...other, z: zero, min: 0, max: 0 };
}

/**
 * Multi Sided View: `field` (this side's model, its zero plane `depth` under
 * the top) with the other side's model, already turned over onto the same
 * cells, as its underside. The other side's model stands on the same
 * modelling plane facing down, so a height `h` there lies `h` under the
 * plane. Where the two sides overlap (only where a side dips below the
 * plane) the cell is cut through; where neither side has a model the plane
 * shows as a sheet.
 */
export function withUnderside(field: Heightfield, other: Composite, depth: number): Heightfield {
  if (other.width !== field.width || other.height !== field.height) {
    return field;
  }
  const plane = field.top - depth;
  const cells = field.z.data.length;
  const floor = new Float32Array(cells);
  const z = new Float32Array(cells);
  const topOf = heightReader(field.z);
  const otherOf = heightReader(other.z);
  for (let i = 0; i < cells; i += 1) {
    const h = otherOf(i);
    const under = plane - (Number.isNaN(h) ? 0 : h);
    const top = topOf(i);
    z[i] = top;
    if (under > top) {
      z[i] = Number.NaN;
      floor[i] = Number.NaN;
    } else {
      floor[i] = under;
    }
  }
  return { ...field, z: floatHeights(z), floor: floatHeights(floor) };
}
