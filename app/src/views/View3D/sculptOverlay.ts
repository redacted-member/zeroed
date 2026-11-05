// The picture laid over the model while sculpting: the bitmap selected
// when the Sculpt form opened, drawn onto the model's cells as colours
// with the chosen opacity.

import type { Point } from '../../bridge/document';
import type { PaletteColours } from '../../bridge/componentTree';
import { imageAffine, type Affine } from '../View2D/bitmapLayer';

/** The cells of a model grid: size and where the first cell's centre is. */
export interface CellGrid {
  width: number;
  height: number;
  originX: number;
  originY: number;
  cellSize: number;
}

/** The transform from image pixels to cell indices (column, row) of `grid`
 * for a bitmap with corners `quad` (lower left, lower right, upper right,
 * upper left); null when the quad is not whole. */
export function imageToCells(
  grid: CellGrid,
  quad: readonly Point[],
  imageWidth: number,
  imageHeight: number,
): Affine | null {
  const run = new Float32Array(quad.flatMap((p) => [p.x, p.y]));
  const toJob = imageAffine(run, imageWidth, imageHeight);
  if (toJob === null || grid.cellSize <= 0) {
    return null;
  }
  const k = 1 / grid.cellSize;
  const [a, b, c, d, e, f] = toJob;
  // Cell (i, j) covers job x from originX + (i - 0.5) cell; so i = (x -
  // originX) / cell + 0.5, and the same for rows.
  return [a * k, b * k, c * k, d * k, (e - grid.originX) * k + 0.5, (f - grid.originY) * k + 0.5];
}

/** Levels per colour channel of the overlay's palette (4096 colours, so
 * the palette stays a small texture). */
const LEVELS = 16;

/** Palette entry of a pixel: 0 for a clear one, else its colour to four
 * bits a channel, plus one. */
export function paletteEntry(r: number, g: number, b: number, a: number): number {
  if (a < 128) {
    return 0;
  }
  const q = (v: number) => Math.min(LEVELS - 1, Math.floor((v * LEVELS) / 256));
  return 1 + (q(r) * LEVELS + q(g)) * LEVELS + q(b);
}

/** The overlay palette: entry 0 clear, the rest every quantised colour at
 * `opacity`. */
export function overlayPalette(opacity: number): Uint8Array {
  const count = LEVELS * LEVELS * LEVELS + 1;
  const palette = new Uint8Array(count * 4);
  const alpha = Math.round(Math.min(1, Math.max(0, opacity)) * 255);
  const level = (q: number) => Math.round((q * 255) / (LEVELS - 1));
  for (let r = 0; r < LEVELS; r += 1) {
    for (let g = 0; g < LEVELS; g += 1) {
      for (let b = 0; b < LEVELS; b += 1) {
        const at = (1 + (r * LEVELS + g) * LEVELS + b) * 4;
        palette[at] = level(r);
        palette[at + 1] = level(g);
        palette[at + 2] = level(b);
        palette[at + 3] = alpha;
      }
    }
  }
  return palette;
}

/** The picture as colours of the grid's cells, `opacity` (0 to 1) of the
 * way over the model; cells it misses stay clear. */
export function overlayColours(
  grid: CellGrid,
  image: ImageBitmap,
  quad: readonly Point[],
  opacity: number,
): PaletteColours | null {
  const transform = imageToCells(grid, quad, image.width, image.height);
  if (transform === null || grid.width <= 0 || grid.height <= 0) {
    return null;
  }
  const canvas = new OffscreenCanvas(grid.width, grid.height);
  const context = canvas.getContext('2d');
  if (context === null) {
    return null;
  }
  context.setTransform(...transform);
  context.imageSmoothingEnabled = true;
  context.drawImage(image, 0, 0);
  const pixels = context.getImageData(0, 0, grid.width, grid.height).data;
  const index = new Uint16Array(grid.width * grid.height);
  for (let i = 0; i < index.length; i += 1) {
    index[i] = paletteEntry(
      pixels[i * 4] ?? 0,
      pixels[i * 4 + 1] ?? 0,
      pixels[i * 4 + 2] ?? 0,
      pixels[i * 4 + 3] ?? 0,
    );
  }
  return {
    width: grid.width,
    height: grid.height,
    palette: overlayPalette(opacity),
    index,
    any: true,
  };
}
