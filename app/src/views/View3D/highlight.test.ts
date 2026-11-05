import { describe, expect, it } from 'vitest';

import type { PaletteColours } from '../../bridge/componentTree';
import { colourAt } from './componentColours';
import { HIGHLIGHT_HIDDEN, HIGHLIGHT_SEEN, tintColours } from './highlight';

/** The RGBA colour cell `i` shows. */
function px(colours: PaletteColours | null, i: number): number[] {
  if (colours === null) {
    return [];
  }
  const at = colourAt(colours, i);
  return Array.from(colours.palette.slice(at, at + 4));
}

describe('selection highlight', () => {
  const mask = (marks: number[]) => ({ width: 3, height: 1, mask: new Uint8Array(marks) });

  it('tints seen cells red and hidden cells green over the material', () => {
    const out = tintColours(null, mask([HIGHLIGHT_SEEN, HIGHLIGHT_HIDDEN, 0]));
    expect(out).not.toBeNull();
    const [r0 = 0, g0 = 0, , a0 = 0] = px(out, 0);
    const [r1 = 0, g1 = 0] = px(out, 1);
    expect(r0).toBeGreaterThan(g0);
    expect(g1).toBeGreaterThan(r1);
    expect(a0).toBeGreaterThan(0);
    expect(a0).toBeLessThan(255);
    expect(px(out, 2)[3]).toBe(0);
  });

  it('mixes the tint into cells that have a colour and keeps the input', () => {
    const palette = new Uint8Array([0, 0, 0, 0, 0, 0, 255, 255]);
    const colours = { width: 3, height: 1, palette, index: new Uint8Array([1, 1, 1]), any: true };
    const out = tintColours(colours, mask([HIGHLIGHT_SEEN, 0, 0]));
    const [r = 0, , b = 0, a = 0] = px(out, 0);
    expect(r).toBeGreaterThan(0);
    expect(b).toBeLessThan(255);
    expect(a).toBe(255);
    expect(px(out, 1)).toEqual([0, 0, 255, 255]);
    expect(Array.from(palette)).toEqual([0, 0, 0, 0, 0, 0, 255, 255]);
  });

  it('leaves the colours alone with no marks or another grid', () => {
    const colours = {
      width: 3,
      height: 1,
      palette: new Uint8Array(4),
      index: new Uint8Array(3),
      any: false,
    };
    expect(tintColours(colours, null)).toBe(colours);
    expect(tintColours(colours, mask([0, 0, 0]))).toBe(colours);
    expect(tintColours(null, null)).toBeNull();
    const other = { width: 2, height: 1, mask: new Uint8Array([1, 1]) };
    expect(tintColours(colours, other)).toBe(colours);
  });
});
