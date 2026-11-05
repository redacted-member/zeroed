import { describe, expect, it } from 'vitest';

import { imageToCells, overlayPalette, paletteEntry } from './sculptOverlay';

describe('sculpt overlay', () => {
  it('maps image pixels onto the model cells', () => {
    const grid = { width: 100, height: 50, originX: 0.5, originY: 0.5, cellSize: 1 };
    // A 20 by 10 mm bitmap from (10, 5), drawn from 200 by 100 pixels.
    const quad = [
      { x: 10, y: 5 },
      { x: 30, y: 5 },
      { x: 30, y: 15 },
      { x: 10, y: 15 },
    ];
    const t = imageToCells(grid, quad, 200, 100);
    expect(t).not.toBeNull();
    if (t !== null) {
      const [a, b, c, d, e, f] = t;
      const at = (x: number, y: number) => [a * x + c * y + e, b * x + d * y + f];
      // The image's top left is the bitmap's upper left corner (10, 15):
      // cell column 10, row 15 edges.
      expect(at(0, 0)).toEqual([10, 15]);
      expect(at(200, 100)).toEqual([30, 5]);
    }
    expect(imageToCells(grid, quad.slice(0, 2), 200, 100)).toBeNull();
  });

  it('quantises the picture into a small palette at the chosen opacity', () => {
    expect(paletteEntry(10, 10, 10, 0)).toBe(0);
    const white = paletteEntry(255, 255, 255, 255);
    expect(white).toBe(4096);
    const palette = overlayPalette(0.5);
    expect(palette.length).toBe(4097 * 4);
    expect(Array.from(palette.slice(white * 4, white * 4 + 4))).toEqual([255, 255, 255, 128]);
    expect(palette[3]).toBe(0);
  });
});
