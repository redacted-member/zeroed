// Shapes of the model commands that cut the relief up or read it back as
// vectors: cross section, slice and split. Lengths are millimetres.

import type { Point } from './document';

export interface CrossSectionArgs {
  from: Point;
  to: Point;
  /** Multiplies the heights along Y; omitted draws them true to size. */
  scale?: number;
  /** Where the section starts; omitted for just above the job. */
  origin?: Point;
}

export interface CrossSectionReply {
  id: number;
  /** Length of the section line. */
  length: number;
  maxHeight: number;
}

/** One Z band of a slice, above the model's zero plane. */
export interface Slab {
  /** 1 for the bottom slab. */
  index: number;
  bottom: number;
  top: number;
}

export interface SliceArgs {
  /** Slab thicknesses bottom up (top down with fromTop); the last one
   * repeats. Ignored when count is given. */
  thicknesses: number[];
  /** Equal slabs, this many, in place of the thicknesses. */
  count?: number;
  /** Takes the thicknesses down from the model's top, the part slab at the
   * bottom. */
  fromTop?: boolean;
  /** Outlines each slab with closed vectors on a layer named after it. */
  boundaryVectors?: boolean;
  /** Components to slice; omitted for the whole visible model. */
  componentIds?: number[];
  /** Name the slices are numbered after. */
  name?: string;
  /** The Slice Heights list: Z values to cut at, in place of the
   * thicknesses and the count. */
  heights?: number[];
}

export interface SlicePreview {
  maxHeight: number;
  slabs: Slab[];
}

export interface SliceReply {
  ids: number[];
  /** Components hidden because the slices replace them. */
  hidden: number[];
  slabs: Slab[];
  /** Boundary vectors made, empty unless asked for. */
  vectors: number[];
  /** The layers the boundary vectors went on, one per slab. */
  layers: number[];
}

export type SplitMode = 'split' | 'clearInside' | 'clearOutside';

export interface SplitArgs {
  componentId: number;
  vectorIds: number[];
  mode: SplitMode;
}

export interface SplitReply {
  id: number;
  /** The new component holding the inside part, split mode only. */
  insideId?: number;
}
