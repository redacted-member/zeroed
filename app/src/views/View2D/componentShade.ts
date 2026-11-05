// Shading of the 2D component preview: the composite model as grey
// pixels lit from the upper left. Kept apart from the layer so the shading
// worker loads only this and the height codes.

import { HEIGHT_NAN, type Heights } from '../../bridge/heights';
import type { Composite } from '../../bridge/model';

/** Light direction (from the upper left, above), normalised. */
const LIGHT: readonly [number, number, number] = (() => {
  const v: [number, number, number] = [-1, 1, 1.4];
  const n = Math.hypot(...v);
  return [v[0] / n, v[1] / n, v[2] / n];
})();

/** Heights closer than this to zero with a flat surround count as no model. */
const EMPTY_EPSILON = 1e-6;

/** Row `j` of the heights into `into`, in millimetres, NaN where there is
 * no material. Decoded as `heightReader` does, one row at a time so
 * the shading loop reads plain numbers. */
function readRow(heights: Heights, width: number, j: number, into: Float64Array): void {
  const start = j * width;
  if (heights.code === null) {
    const data = heights.data;
    for (let i = 0; i < width; i += 1) {
      into[i] = data[start + i] ?? Number.NaN;
    }
    return;
  }
  const data = heights.data;
  const { base, step } = heights.code;
  for (let i = 0; i < width; i += 1) {
    const q = data[start + i];
    into[i] = q === undefined || q === HEIGHT_NAN ? Number.NaN : base + q * step;
  }
}

/**
 * RGBA pixels, top row first, of the model shaded by a light from the upper
 * left. Cells without model (empty, or on the zero plane with no slope) are
 * transparent. Written into `out` when given (an ImageData's pixels, all
 * zero), which saves a copy of the whole picture.
 */
export function shadeComposite(
  composite: Pick<Composite, 'width' | 'height' | 'cellSize' | 'z'>,
  out: Uint8ClampedArray = new Uint8ClampedArray(composite.width * composite.height * 4),
): Uint8ClampedArray {
  const { width, height, cellSize } = composite;
  const cell = cellSize > 0 ? cellSize : 1;
  if (width === 0 || height === 0) {
    return out;
  }
  // Rows j - 1, j and j + 1, clamped at the edges as the grid ends there.
  let below = new Float64Array(width);
  let here = new Float64Array(width);
  let above = new Float64Array(width);
  readRow(composite.z, width, 0, here);
  below.set(here);
  if (height > 1) {
    readRow(composite.z, width, 1, above);
  } else {
    above.set(here);
  }
  const zero = (h: number) => (h !== h ? 0 : h);
  const last = width - 1;
  for (let j = 0; j < height; j += 1) {
    const row = height - 1 - j;
    for (let i = 0; i < width; i += 1) {
      const raw = here[i] as number;
      if (raw !== raw) {
        continue;
      }
      const left = zero(here[i > 0 ? i - 1 : 0] as number);
      const right = zero(here[i < last ? i + 1 : last] as number);
      const down = zero(below[i] as number);
      const up = zero(above[i] as number);
      const dx = (right - left) / (2 * cell);
      const dy = (up - down) / (2 * cell);
      if (
        Math.abs(raw) < EMPTY_EPSILON &&
        Math.abs(dx) < EMPTY_EPSILON &&
        Math.abs(dy) < EMPTY_EPSILON
      ) {
        continue;
      }
      // Surface normal (-dx, -dy, 1), normalised, against the light.
      const n = Math.hypot(dx, dy, 1);
      const lambert = (-dx * LIGHT[0] - dy * LIGHT[1] + LIGHT[2]) / n;
      const grey = Math.round(70 + 170 * Math.max(0, Math.min(1, lambert)));
      const o = (row * width + i) * 4;
      out[o] = grey;
      out[o + 1] = grey;
      out[o + 2] = grey;
      out[o + 3] = 255;
    }
    // Move the three rows up one; the new top row repeats at the top edge.
    const spare = below;
    below = here;
    here = above;
    above = spare;
    if (j + 2 < height) {
      readRow(composite.z, width, j + 2, above);
    } else {
      above.set(here);
    }
  }
  return out;
}
