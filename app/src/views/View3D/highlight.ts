// The selection highlight in the 3D view: the kernel marks, per model
// cell, where the selected components show (tinted red) and where other
// components hide them (tinted green). The tint is laid over whatever
// colours the model already has, as a longer palette, so both render paths
// draw it as they draw component colours.

import type { PaletteColours } from '../../bridge/componentTree';
import { colourAt } from './componentColours';

/** A cell where a selected component shows. */
export const HIGHLIGHT_SEEN = 1;
/** A cell where other components hide a selected one. */
export const HIGHLIGHT_HIDDEN = 2;

/** What `model.highlight` answers: one mark per model cell. */
export interface HighlightMask {
  width: number;
  height: number;
  mask: Uint8Array;
}

const SEEN_TINT = [220, 30, 30] as const;
const HIDDEN_TINT = [40, 190, 60] as const;
/** How much of the tint shows over the colour below, 0 to 1. */
const TINT_AMOUNT = 0.55;

/**
 * The cell colours with the highlight laid over them, or the colours as
 * they are when nothing is highlighted or the mask is for another grid.
 * Cells without a colour of their own get the tint part way over the
 * material (by alpha); coloured cells are mixed with it. The result is a
 * palette three times as long: each colour as it is, tinted as seen, and
 * tinted as hidden.
 */
export function tintColours(
  colours: PaletteColours | null,
  highlight: HighlightMask | null,
): PaletteColours | null {
  if (highlight === null || !highlight.mask.some((m) => m !== 0)) {
    return colours;
  }
  const { width, height, mask } = highlight;
  const cells = width * height;
  if (
    mask.length < cells ||
    (colours !== null &&
      (colours.width !== width || colours.height !== height || colours.index.length < cells))
  ) {
    return colours;
  }
  const base = colours?.palette ?? new Uint8Array(4);
  const entries = Math.max(base.length / 4, 1);
  const palette = new Uint8Array(entries * 3 * 4);
  palette.set(base.subarray(0, entries * 4));
  for (const [block, tint] of [
    [1, SEEN_TINT],
    [2, HIDDEN_TINT],
  ] as const) {
    for (let entry = 0; entry < entries; entry += 1) {
      const from = entry * 4;
      const at = (block * entries + entry) * 4;
      if ((base[from + 3] ?? 0) === 0) {
        palette.set([tint[0], tint[1], tint[2], Math.round(TINT_AMOUNT * 255)], at);
        continue;
      }
      for (let k = 0; k < 3; k += 1) {
        const own = base[from + k] ?? 0;
        palette[at + k] = Math.round(own + (tint[k] - own) * TINT_AMOUNT);
      }
      palette[at + 3] = 255;
    }
  }
  const index = new Uint16Array(cells);
  for (let i = 0; i < cells; i += 1) {
    const own = colours === null ? 0 : colourAt(colours, i) / 4;
    const mark = mask[i];
    const block = mark === HIGHLIGHT_SEEN ? 1 : mark === HIGHLIGHT_HIDDEN ? 2 : 0;
    index[i] = block * entries + own;
  }
  return { width, height, palette, index, any: true };
}
