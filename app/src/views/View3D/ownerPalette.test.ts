import { describe, expect, it } from 'vitest';

import { machinedColours } from '../../forms/PreviewToolpathsForm/model';
import { gpuFits } from './gpuSurface';
import type { PaletteColours } from '../../bridge/componentTree';
import { colourAt } from './componentColours';
import { machinedPalette, ownerColours, paletteEntry } from './ownerPalette';

/** One RGBA colour per cell, as the preview form spells them out. */
function expand(colours: PaletteColours | null): Uint8Array | null {
  if (colours === null) {
    return null;
  }
  const rgba = new Uint8Array(colours.index.length * 4);
  for (let i = 0; i < colours.index.length; i += 1) {
    const at = colourAt(colours, i);
    rgba.set(colours.palette.subarray(at, at + 4), i * 4);
  }
  return rgba;
}

const colourOf = (owner: number) => (owner === 1 ? '#ff0000' : owner === 2 ? null : '#00ff00');

describe('machined colour palette', () => {
  it('is null while the material shows', () => {
    expect(machinedPalette(3, 'material', '#123456', colourOf)).toBeNull();
  });

  it('gives each toolpath its colour and leaves the rest uncoloured', () => {
    const palette = machinedPalette(3, 'toolpath', '#123456', colourOf);
    expect(palette).not.toBeNull();
    const rgba = palette?.rgba ?? new Uint8Array(0);
    // Uncut, three toolpaths and the entry beyond them.
    expect(rgba.length).toBe(5 * 4);
    expect(Array.from(rgba.slice(0, 4))).toEqual([0, 0, 0, 0]);
    expect(Array.from(rgba.slice(4, 8))).toEqual([255, 0, 0, 255]);
    expect(rgba[2 * 4 + 3]).toBe(0);
    expect(Array.from(rgba.slice(12, 16))).toEqual([0, 255, 0, 255]);
    expect(rgba[4 * 4 + 3]).toBe(0);
  });

  it('paints every machined cell with a global fill, owners beyond the list too', () => {
    const palette = machinedPalette(1, 'global', '#102030', colourOf);
    const rgba = palette?.rgba ?? new Uint8Array(0);
    expect(Array.from(rgba.slice(4, 8))).toEqual([16, 32, 48, 255]);
    expect(Array.from(rgba.slice(8, 12))).toEqual([16, 32, 48, 255]);
    expect(paletteEntry({ rgba }, 7)).toBe(8);
  });

  it('expands to the same cell colours as the preview form', () => {
    const field = { width: 3, height: 2, owner: Uint16Array.from([0, 1, 2, 3, 9, 1]) };
    for (const mode of ['toolpath', 'global', 'surface'] as const) {
      const palette = machinedPalette(3, mode, '#abcdef', colourOf);
      const expected = machinedColours(field, mode, '#abcdef', (owner) =>
        owner <= 3 ? colourOf(owner) : null,
      );
      expect(palette).not.toBeNull();
      expect(expand(ownerColours(field, palette ?? { rgba: new Uint8Array(0) }))).toEqual(
        expected?.rgba,
      );
    }
  });

  it('has no cell colours when the field has no owners', () => {
    const palette = machinedPalette(1, 'global', '#abcdef', colourOf);
    expect(
      ownerColours(
        { width: 2, height: 1, owner: new Uint16Array(0) },
        palette ?? { rgba: new Uint8Array(0) },
      ),
    ).toBeNull();
  });
});

describe('GPU surface limits', () => {
  const limits = { maxTextureSize: 4096, vertexTextures: 16 };
  it('takes fields that fit a texture', () => {
    expect(gpuFits({ width: 4096, height: 10 }, limits)).toBe(true);
  });
  it('falls back for fields too big, too small or without vertex textures', () => {
    expect(gpuFits({ width: 4097, height: 10 }, limits)).toBe(false);
    expect(gpuFits({ width: 1, height: 10 }, limits)).toBe(false);
    expect(gpuFits({ width: 10, height: 10 }, { ...limits, vertexTextures: 0 })).toBe(false);
  });
});

describe('Surface Color', () => {
  it('paints only the uncut surface', () => {
    const palette = machinedPalette(2, 'surface', '#102030', () => '#ffffff');
    expect(palette?.rgba.slice(0, 4)).toEqual(new Uint8Array([16, 32, 48, 255]));
    expect(palette?.rgba.slice(4, 8)).toEqual(new Uint8Array([0, 0, 0, 0]));
    expect(machinedPalette(2, 'surface', 'nope', () => null)).toBeNull();
  });
});
