// Machined area colours of a toolpath preview as a palette indexed by the
// cell owner (toolpath list position plus one), so the colours only change
// when the fill settings do. Both the GPU surface and the CPU mesh look
// owners up in it, as they do component colours.

import type { Heightfield } from '../../bridge/cam';
import type { PaletteColours } from '../../bridge/componentTree';
import type { FillMode } from '../../forms/PreviewToolpathsForm/model';
import { hasColours } from './componentColours';

/**
 * RGBA bytes per owner: entry 0 is uncut cells (coloured only by Surface
 * Color), entries 1
 * to `count` the toolpaths, and one last entry for any owner beyond them.
 * Alpha 0 leaves the material showing.
 */
export interface OwnerPalette {
  rgba: Uint8Array;
}

/** `#rrggbb` as bytes, or null when it is not one. */
function bytes(colour: string): [number, number, number] | null {
  const match = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(colour);
  if (match === null) {
    return null;
  }
  return [
    parseInt(match[1] ?? '0', 16),
    parseInt(match[2] ?? '0', 16),
    parseInt(match[3] ?? '0', 16),
  ];
}

/**
 * The palette for `count` toolpaths under a fill mode, or null when the
 * material shows everywhere. `global` is the one colour of Surface Color or
 * Global Fill Color. `colourOf` gives a toolpath's colour by owner
 * (1 based), null for none.
 */
export function machinedPalette(
  count: number,
  mode: FillMode,
  global: string,
  colourOf: (owner: number) => string | null,
): OwnerPalette | null {
  if (mode === 'material') {
    return null;
  }
  const entries = count + 2;
  const rgba = new Uint8Array(entries * 4);
  if (mode === 'surface') {
    // The uncut surface is painted; the machined areas show the material.
    const surface = bytes(global);
    if (surface === null) {
      return null;
    }
    rgba.set([surface[0], surface[1], surface[2], 255], 0);
    return { rgba };
  }
  const one = bytes(global);
  let any = false;
  for (let owner = 1; owner < entries; owner += 1) {
    let colour = one;
    if (mode === 'toolpath') {
      const chosen = owner <= count ? colourOf(owner) : null;
      colour = chosen === null ? null : bytes(chosen);
    }
    if (colour === null) {
      continue;
    }
    rgba.set([colour[0], colour[1], colour[2], 255], owner * 4);
    any = true;
  }
  return any ? { rgba } : null;
}

/** The palette's colour for a cell owner, as an index into `rgba`. */
export function paletteEntry(palette: OwnerPalette, owner: number): number {
  const last = palette.rgba.length / 4 - 1;
  return Math.min(owner, last) * 4;
}

/**
 * The field's owners as palette colours, the shape the CPU mesh paints:
 * each cell takes the palette entry of its owner. Null when the field has
 * no owners or no cell gets a colour.
 */
export function ownerColours(
  field: Pick<Heightfield, 'width' | 'height' | 'owner'>,
  palette: OwnerPalette,
): PaletteColours | null {
  if (field.owner.length !== field.width * field.height) {
    return null;
  }
  const colours = {
    width: field.width,
    height: field.height,
    palette: palette.rgba,
    index: field.owner,
    any: true,
  };
  return hasColours(colours) ? colours : null;
}
