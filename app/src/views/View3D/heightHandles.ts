// The height handles of a component in the 3D view, on a post standing
// out to the right of the front right corner of its box: Base Height raises the flat riser
// under the shape, Shape Height scales the shape above the riser, and
// Total Height, above the top, scales both together. Heights are
// millimetres above the model's zero plane.

import type { ComponentSummary } from '../../bridge/model';
import type { XY } from './transformHandles';

export type HeightKind = 'base' | 'shape' | 'total';

export const HEIGHT_KINDS: readonly HeightKind[] = ['base', 'shape', 'total'];

/** A component's heights: the riser, the shape above it and its height
 * scale (1 is 100 %). */
export interface Heights {
  base: number;
  shape: number;
  scale: number;
}

/** The smallest scale a drag leaves, so a shape can always grow back. */
const MIN_FACTOR = 0.01;

export function heightsOf(component: ComponentSummary): Heights {
  const base = component.baseHeight ?? 0;
  const top = component.maxHeight ?? base;
  return { base, shape: Math.max(0, top - base), scale: component.heightScale };
}

/** How far the Total Height handle stands above the top, for a box whose
 * larger side is `span` millimetres. */
export function totalReach(heights: Heights, span: number): number {
  return Math.max(2, (heights.base + heights.shape) * 0.25, span * 0.12);
}

/** Height of a handle above the zero plane. */
export function heightOf(kind: HeightKind, heights: Heights, span = 0): number {
  const top = heights.base + heights.shape;
  switch (kind) {
    case 'base':
      return heights.base;
    case 'shape':
      return top;
    case 'total':
      return top + totalReach(heights, span);
  }
}

/** Heights with the shape scaled by `factor`. */
function scaled(heights: Heights, factor: number): Heights {
  return { ...heights, shape: heights.shape * factor, scale: heights.scale * factor };
}

/** The heights a drag of `dz` millimetres on a handle asks for. */
export function dragHeights(kind: HeightKind, start: Heights, dz: number): Heights {
  switch (kind) {
    case 'base':
      return { ...start, base: Math.max(0, start.base + dz) };
    case 'shape': {
      if (start.shape < 1e-9) {
        return start;
      }
      return scaled(start, Math.max(MIN_FACTOR, (start.shape + dz) / start.shape));
    }
    case 'total': {
      const total = start.base + start.shape;
      if (total < 1e-9) {
        return start;
      }
      const factor = Math.max(MIN_FACTOR, (total + dz) / total);
      return { ...scaled(start, factor), base: start.base * factor };
    }
  }
}

/** Heights from a typed value: Base and Total Height in millimetres, Shape
 * Height in percent of the component's own height. */
export function typedHeights(kind: HeightKind, start: Heights, value: number): Heights | null {
  if (!Number.isFinite(value)) {
    return null;
  }
  switch (kind) {
    case 'base':
      return value < 0 ? null : { ...start, base: value };
    case 'shape':
      if (value <= 0 || start.scale <= 0) {
        return null;
      }
      return scaled(start, value / 100 / start.scale);
    case 'total': {
      const total = start.base + start.shape;
      return value <= 0 || total < 1e-9 ? null : dragHeights('total', start, value - total);
    }
  }
}

/** The `model.props` fields that change from `start` to `now`. */
export function heightPatch(
  start: Heights,
  now: Heights,
): { heightScale?: number; baseHeight?: number } {
  const patch: { heightScale?: number; baseHeight?: number } = {};
  if (Math.abs(now.scale - start.scale) > 1e-9) {
    patch.heightScale = now.scale;
  }
  if (Math.abs(now.base - start.base) > 1e-9) {
    patch.baseHeight = now.base;
  }
  return patch;
}

export function sameHeights(a: Heights, b: Heights): boolean {
  return Object.keys(heightPatch(a, b)).length === 0;
}

/** Millimetres a pointer moved along the post: `up` is where one
 * millimetre above `foot` lands on the screen. */
export function riseAlong(foot: XY, up: XY, from: XY, to: XY): number {
  const ux = up.x - foot.x;
  const uy = up.y - foot.y;
  const length = ux * ux + uy * uy;
  if (length < 1e-12) {
    return 0;
  }
  return ((to.x - from.x) * ux + (to.y - from.y) * uy) / length;
}
