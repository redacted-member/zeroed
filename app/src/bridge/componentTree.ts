// Shapes of the component tree commands: groups inside levels, component
// properties, drag and drop moves and level clipping. Lengths are
// millimetres.

import type { BlobHeader } from './types';
import type { Combine, ComponentTransformParams, FadeProp, TiltProp } from './model';

/** Components inside a level that bake together, then meet the rest of the
 * level with the group's combine mode, and hide and move as one. */
export interface ComponentGroup {
  id: number;
  name: string;
  visible: boolean;
  combine: Combine;
  /** Open in the tree. */
  expanded: boolean;
  /** `#rrggbb` for every member, or null to take the members' colours. */
  color: string | null;
  /** Preview material `color` was taken from. */
  material?: string | null;
}

/** What `model.props` changes; absent fields are left alone. */
export interface ComponentProps {
  /** Only when one component is chosen. */
  name?: string;
  visible?: boolean;
  combine?: Combine;
  /** Shape height: 1 is 100 %. */
  heightScale?: number;
  /** Flat riser under the whole component, mm. */
  baseHeight?: number;
  /** `#rrggbb`, or null for its parent's appearance. */
  color?: string | null;
  /** Preview material the colour was taken from, or null. */
  material?: string | null;
  /** Dynamic fade, or null to turn it off. */
  fade?: FadeProp | null;
  /** Dynamic tilt, or null to turn it off. */
  tilt?: TiltProp | null;
  /** Bake the dynamic properties into the heights and reset them. */
  bake?: boolean;
}

export interface GroupPatch {
  name?: string;
  visible?: boolean;
  combine?: Combine;
  expanded?: boolean;
  color?: string | null;
  material?: string | null;
}

/** Where a drag and drop puts components: a level, optionally a group in
 * it, and just below (`before`) or above (`after`) a component, else on top. */
export interface TreeTarget {
  level: number;
  group?: number;
  before?: number;
  after?: number;
}

export type GroupTransformParams = ComponentTransformParams;

export interface ColorsHeader extends BlobHeader {
  revision: number;
  width: number;
  height: number;
  /** True when any cell has a colour. */
  any: boolean;
  /** The preview material of each palette entry, null for a plain colour. */
  materials?: (string | null)[];
}

/** Selection highlight marks per composite cell: 0 none, 1 the component
 * shows there, 2 other components hide it there. */
export interface HighlightReply {
  width: number;
  height: number;
  mask: Uint8Array;
}

/**
 * Colours per composite cell as a palette: `palette` holds RGBA bytes per
 * entry (entry 0 is clear, the material shows), `index` the entry of each
 * cell. An index past the last entry takes the last one.
 */
export interface PaletteColours {
  width: number;
  height: number;
  palette: Uint8Array;
  index: Uint8Array | Uint16Array;
  /** True when any cell has a colour. */
  any: boolean;
  /** The preview material each palette entry was taken from, null for a
   * plain colour; absent when none was. */
  materials?: (string | null)[];
}

/** Colours per composite cell, RGBA, alpha 0 for the material. */
export interface ModelColors {
  width: number;
  height: number;
  rgba: Uint8Array;
  /** True when any cell has a colour; absent from older replies. */
  any?: boolean;
}
