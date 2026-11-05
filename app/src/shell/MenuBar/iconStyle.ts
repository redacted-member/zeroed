// How a menu icon looks outside the page: the native menu draws icons from
// color images, so the SVG gets the look of shell/ToolIcons/ToolIcons.scss
// as presentation attributes, with the colors of the current appearance,
// before it is rasterized. Pure, so the mapping is tested.

import { ICON_FLAGS } from '../ToolIcons/iconSpec';

/** Side of a native menu icon in pixels: 18 points at twice the pixel
 * density (ICON_PX in app/src-tauri/src/menu.rs). */
export const ICON_PX = 36;

/**
 * The colors an icon draws with, by custom property name of
 * styles/_tokens.scss: `text` for the neutral parts and the `icon-*`
 * tokens of the roles and surfaces.
 */
export type IconPalette = Readonly<Record<string, string>>;

/** The tokens an icon palette holds. */
export const PALETTE_TOKENS: readonly string[] = [
  'text',
  ...Object.values(ICON_FLAGS).flatMap((flag) => (flag.kind === 'style' ? [] : [flag.token])),
];

/** The root SVG's attributes, as `.tool-icon` sets them, drawn in the palette's text color. */
export function rootPresentation(palette: IconPalette): Record<string, string> {
  return {
    color: palette['text'] ?? '#000',
    fill: 'none',
    stroke: 'currentColor',
    'stroke-width': '1.5',
    'stroke-linecap': 'round',
    'stroke-linejoin': 'round',
  };
}

/** The attributes the ToolIcons line style classes set on a shape. */
const STYLE_PRESENTATION: Readonly<Record<string, Readonly<Record<string, string>>>> = {
  'tool-icon__thin': { 'stroke-width': '1' },
  'tool-icon__dash': { 'stroke-dasharray': '2 2.5' },
  'tool-icon__fill': { fill: 'currentColor', stroke: 'none' },
  'tool-icon__tint': { fill: 'currentColor', 'fill-opacity': '0.3' },
};

function classPresentation(name: string, palette: IconPalette): Record<string, string> {
  const style = STYLE_PRESENTATION[name];
  if (style !== undefined) {
    return { ...style };
  }
  const flag = Object.values(ICON_FLAGS).find((each) => each.className === name);
  if (flag === undefined || flag.kind === 'style') {
    return {};
  }
  const color = palette[flag.token];
  if (color === undefined) {
    return {};
  }
  return flag.kind === 'role' ? { color } : { fill: color };
}

/** The presentation attributes for a shape with the class list `classes`. */
export function presentation(classes: string, palette: IconPalette): Record<string, string> {
  return classes
    .split(/\s+/)
    .reduce<Record<string, string>>(
      (out, name) => ({ ...out, ...classPresentation(name, palette) }),
      {},
    );
}
