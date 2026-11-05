// The clipboard's contents live in the kernel (clipboard.copy and
// clipboard.paste); this store only knows how many objects a paste adds, for
// the menus. Pasted copies are offset so they never sit on the originals.

import { create } from 'zustand';

interface ClipboardState {
  /** Objects a paste would add; 0 when nothing was copied. */
  count: number;
  setCount: (count: number) => void;
}

export const useClipboardStore = create<ClipboardState>()((set) => ({
  count: 0,
  setCount: (count) => set({ count }),
}));

/** Offset applied to pasted and duplicated copies, in millimetres. */
export const PASTE_OFFSET_MM = 10;
