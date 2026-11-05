import { describe, expect, it } from 'vitest';

import { floatHeights } from '../../bridge/heights';
import { previewAffine, shadeComposite } from './componentPreview';

describe('2D component preview', () => {
  it('leaves cells without model transparent and shades the model grey', () => {
    // 3 x 2 grid: a single raised cell at i = 1, j = 1 (the top row).
    const z = new Float32Array([0, 0, 0, 0, 2, Number.NaN]);
    const pixels = shadeComposite({ width: 3, height: 2, cellSize: 1, z: floatHeights(z) });
    // Top row first: the raised cell is pixel (1, 0).
    const raised = (0 * 3 + 1) * 4;
    expect(pixels[raised + 3]).toBe(255);
    expect(pixels[raised]).toBeGreaterThan(0);
    // The NaN cell (i = 2, j = 1) is pixel (2, 0): transparent.
    expect(pixels[(0 * 3 + 2) * 4 + 3]).toBe(0);
    // A zero cell next to the raised one has slope, so it shows.
    expect(pixels[(1 * 3 + 1) * 4 + 3]).toBe(255);
  });

  it('lights slopes facing the upper left brighter than slopes facing away', () => {
    // A ramp rising to the right faces left (lit); falling faces right (dark).
    const rising = new Float32Array([1, 2, 3, 1, 2, 3, 1, 2, 3]);
    const falling = new Float32Array([3, 2, 1, 3, 2, 1, 3, 2, 1]);
    const lit = shadeComposite({ width: 3, height: 3, cellSize: 1, z: floatHeights(rising) });
    const dark = shadeComposite({ width: 3, height: 3, cellSize: 1, z: floatHeights(falling) });
    const centre = (1 * 3 + 1) * 4;
    expect(lit[centre] ?? 0).toBeGreaterThan(dark[centre] ?? 0);
  });

  it('places the top row at the highest Y with cell centres on the grid points', () => {
    const [a, b, c, d, e, f] = previewAffine({
      height: 4,
      cellSize: 0.5,
      originX: 10,
      originY: 20,
    });
    expect([a, b, c, d]).toEqual([0.5, 0, 0, -0.5]);
    // Pixel (0.5, 0.5), the centre of the top left pixel, lands on cell (0, 3).
    expect(e + a * 0.5).toBeCloseTo(10);
    expect(f + d * 0.5).toBeCloseTo(20 + 3 * 0.5);
  });
});
