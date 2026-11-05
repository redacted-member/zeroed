// How the material looks in the 3D view: a colour and the surface finish
// three.js needs for each material choice. The grain of the wood and MDF
// materials is drawn from componentFinish.ts.

import type { Material3d } from '../../bridge/prefs';
import type { JobAppearance } from '../../bridge/types';

export interface MaterialLook {
  /** `#rrggbb`. */
  colour: string;
  roughness: number;
  metalness: number;
}

// A touch of sheen so the rim light catches the edges of wood.
const WOOD = { roughness: 0.72, metalness: 0.02 };

const LOOKS: Readonly<Record<Exclude<Material3d, 'theme' | 'solid'>, MaterialLook>> = {
  pine: { colour: '#e2c38e', ...WOOD },
  oak: { colour: '#b98c57', ...WOOD },
  walnut: { colour: '#6a4631', ...WOOD },
  maple: { colour: '#ead8b1', ...WOOD },
  mdf: { colour: '#a88b64', roughness: 0.95, metalness: 0 },
  acrylic: { colour: '#dbe6ec', roughness: 0.25, metalness: 0.05 },
  aluminium: { colour: '#b9bec4', roughness: 0.4, metalness: 0.75 },
};

/** Material choices in the order menus list them. */
export const MATERIALS: readonly Material3d[] = [
  'theme',
  'pine',
  'oak',
  'walnut',
  'maple',
  'mdf',
  'acrylic',
  'aluminium',
  'solid',
];

/**
 * The look for a material choice. `theme` uses the theme's material colour
 * and `solid` the user's solid colour, both with the default wood finish.
 */
export function materialLook(
  material: Material3d,
  themeColour: string,
  solidColour: string,
): MaterialLook {
  if (material === 'theme') {
    return { colour: themeColour, ...WOOD };
  }
  if (material === 'solid') {
    return { colour: solidColour, roughness: 0.7, metalness: 0 };
  }
  return LOOKS[material];
}

/**
 * The material the job is drawn in: its own appearance, else the 3D view
 * preference for new jobs. Color Shaded View does not change it; it only
 * turns the component colours on and off.
 */
export function jobMaterial(
  appearance: JobAppearance | undefined,
  defaults: { material: Material3d; solidColour: string },
): { material: Material3d; colour: string } {
  return {
    material: appearance?.material ?? defaults.material,
    colour: appearance?.colour ?? defaults.solidColour,
  };
}
