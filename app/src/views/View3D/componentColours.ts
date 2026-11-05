// Component colours on the model in the 3D view: the kernel gives a palette
// of RGBA colours (entry 0 clear, the material shows; part way alpha for a
// tint over it) and the entry of each model cell. The CPU mesh takes them
// as vertex colours on its top surface. Walls, base and cells without a
// colour keep the material colour.

import type { Heightfield } from '../../bridge/cam';
import type { PaletteColours } from '../../bridge/componentTree';

/** Linear RGB from 0 to 1, the space three.js keeps vertex colours in. */
export type LinearRgb = readonly [number, number, number];

/** One sRGB byte as a linear value from 0 to 1. */
export function srgbToLinear(byte: number): number {
  const v = byte / 255;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

/** Byte offset of palette entry `entry` in `palette`, the last entry for
 * one past the end (as the GPU surface reads it). */
export function entryOffset(palette: Uint8Array, entry: number): number {
  const last = Math.max(Math.floor(palette.length / 4) - 1, 0);
  return Math.min(entry, last) * 4;
}

/** Byte offset in the palette of cell `cell`'s colour. */
export function colourAt(colours: PaletteColours, cell: number): number {
  return entryOffset(colours.palette, colours.index[cell] ?? 0);
}

/** Whether any cell has a colour of its own. */
export function hasColours(colours: PaletteColours): boolean {
  const { palette, index } = colours;
  const entries = Math.floor(palette.length / 4);
  const shown = new Uint8Array(Math.max(entries, 1));
  let anyShown = false;
  for (let entry = 0; entry < entries; entry += 1) {
    if ((palette[entry * 4 + 3] ?? 0) !== 0) {
      shown[entry] = 1;
      anyShown = true;
    }
  }
  if (!anyShown) {
    return false;
  }
  const last = Math.max(entries - 1, 0);
  for (let i = 0; i < index.length; i += 1) {
    if (shown[Math.min(index[i] ?? 0, last)] === 1) {
      return true;
    }
  }
  return false;
}

/**
 * Per vertex colours (three floats each) for a heightfield mesh with
 * `vertexCount` vertices whose first ones are the top surface samples at
 * field columns `xs` and rows `ys`, row by row. Null when the colours do not
 * cover the field's grid or no cell has one, so the plain material shows.
 */
export function vertexColours(
  field: Pick<Heightfield, 'width' | 'height'>,
  colours: PaletteColours,
  xs: readonly number[],
  ys: readonly number[],
  vertexCount: number,
  material: LinearRgb,
): Float32Array | null {
  const cells = field.width * field.height;
  if (
    colours.width !== field.width ||
    colours.height !== field.height ||
    colours.index.length < cells ||
    !hasColours(colours)
  ) {
    return null;
  }
  const { palette } = colours;
  const out = new Float32Array(vertexCount * 3);
  for (let v = 0; v < vertexCount; v += 1) {
    out.set(material, v * 3);
  }
  let v = 0;
  for (const cy of ys) {
    for (const cx of xs) {
      const at = colourAt(colours, cy * field.width + cx);
      const alpha = (palette[at + 3] ?? 0) / 255;
      if (alpha !== 0 && v < vertexCount) {
        for (let k = 0; k < 3; k += 1) {
          const own = srgbToLinear(palette[at + k] ?? 0);
          out[v * 3 + k] = material[k] + (own - material[k]) * alpha;
        }
      }
      v += 1;
    }
  }
  return out;
}
