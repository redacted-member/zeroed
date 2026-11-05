import { describe, expect, it } from 'vitest';

import { colourAt, hasColours, srgbToLinear, vertexColours } from './componentColours';

const MATERIAL = [0.5, 0.4, 0.3] as const;

/** Colours for a grid with the given palette and cell entries. */
function paletted(width: number, height: number, palette: number[], index: number[]) {
  return {
    width,
    height,
    palette: Uint8Array.from(palette),
    index: Uint8Array.from(index),
    any: true,
  };
}

describe('component colours in 3D', () => {
  it('converts sRGB bytes to linear values', () => {
    expect(srgbToLinear(0)).toBe(0);
    expect(srgbToLinear(255)).toBeCloseTo(1);
    expect(srgbToLinear(128)).toBeCloseTo(0.2158, 3);
  });

  it('colours the top surface samples and keeps the material elsewhere', () => {
    // A 2 by 2 grid; only cell (1, 0) is red.
    const colors = paletted(2, 2, [0, 0, 0, 0, 255, 0, 0, 255], [0, 1, 0, 0]);
    expect(hasColours(colors)).toBe(true);
    const out = vertexColours({ width: 2, height: 2 }, colors, [0, 1], [0, 1], 6, MATERIAL);
    expect(out).not.toBeNull();
    const at = (v: number) => Array.from(out?.slice(v * 3, v * 3 + 3) ?? []);
    expect(at(0)).toEqual(MATERIAL.map((c) => Math.fround(c)));
    expect(at(1)[0]).toBeCloseTo(1);
    expect(at(1)[1]).toBe(0);
    // Wall and base vertices after the surface keep the material colour.
    expect(at(5)).toEqual(MATERIAL.map((c) => Math.fround(c)));
  });

  it('lays a part way colour over the material by its alpha', () => {
    const colors = paletted(1, 1, [0, 0, 0, 0, 255, 0, 0, 255 / 5], [1]);
    const out = vertexColours({ width: 1, height: 1 }, colors, [0], [0], 1, MATERIAL);
    expect(out?.[0]).toBeCloseTo(0.5 + 0.5 * 0.2);
    expect(out?.[1]).toBeCloseTo(0.4 * 0.8);
  });

  it('reads entries past the end of the palette as its last one', () => {
    const colors = paletted(2, 1, [0, 0, 0, 0, 1, 2, 3, 255], [0, 9]);
    expect(colourAt(colors, 0)).toBe(0);
    expect(colourAt(colors, 1)).toBe(4);
  });

  it('gives nothing when no cell has a colour or the grids differ', () => {
    const blank = paletted(2, 2, [0, 0, 0, 0, 255, 0, 0, 255], [0, 0, 0, 0]);
    expect(hasColours(blank)).toBe(false);
    expect(vertexColours({ width: 2, height: 2 }, blank, [0, 1], [0, 1], 4, MATERIAL)).toBeNull();
    const other = paletted(4, 1, [0, 0, 0, 0, 255, 0, 0, 255], [1, 1, 1, 1]);
    expect(vertexColours({ width: 2, height: 2 }, other, [0, 1], [0, 1], 4, MATERIAL)).toBeNull();
  });
});
