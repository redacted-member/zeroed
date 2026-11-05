// What the node editing tool is over and its node Properties box: the right
// click menu differs over a node and over a span, and X on one node (or
// Properties on the menu) opens its position to type.

import { create } from 'zustand';

import type { Point } from '../bridge/document';

/** What the pointer is over while node editing. */
export type NodeHover = 'node' | 'span' | null;

interface NodeToolState {
  hover: NodeHover;
  setHover: (hover: NodeHover) => void;
  /** The node whose position the Properties box edits, or null when shut. */
  properties: Point | null;
  openProperties: (at: Point) => void;
  closeProperties: () => void;
}

export const useNodeToolStore = create<NodeToolState>()((set, get) => ({
  hover: null,
  setHover: (hover) => {
    if (get().hover !== hover) {
      set({ hover });
    }
  },
  properties: null,
  openProperties: (properties) => set({ properties }),
  closeProperties: () => set({ properties: null }),
}));
