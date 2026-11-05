// Pure state for the Material Appearance dialog: the material list and the
// solid color a draft holds until OK.

import type { Material3d } from '../../bridge/prefs';

export interface MaterialDraft {
  material: Material3d;
  /** `#rrggbb`, used while the material is the solid color. */
  colour: string;
  /** A texture picture (Preferences view3d.textures id) drawn instead of
   * the material; 0 for none. */
  texture?: number;
}

/** The list as the dialog shows it: Use Solid Color first, then the materials. */
export function materialChoices(materials: readonly Material3d[]): Material3d[] {
  const rest = materials.filter((material) => material !== 'solid');
  return materials.includes('solid') ? ['solid', ...rest] : rest;
}

/** Whether the color picker applies to the draft. */
export function usesSolidColour(draft: MaterialDraft): boolean {
  return draft.material === 'solid';
}

/** Picks a list entry; the color is kept for a later switch back to solid.
 * A plain material takes the texture away. */
export function chooseMaterial(draft: MaterialDraft, material: Material3d): MaterialDraft {
  return { ...draft, material, texture: 0 };
}

/** Picks a texture picture; the material stays for when it is removed. */
export function chooseTexture(draft: MaterialDraft, texture: number): MaterialDraft {
  return { ...draft, texture };
}

/** Sets the solid color when it is a `#rrggbb` value; anything else is ignored. */
export function chooseColour(draft: MaterialDraft, colour: string): MaterialDraft {
  return /^#[0-9a-f]{6}$/i.test(colour) ? { ...draft, colour: colour.toLowerCase() } : draft;
}
