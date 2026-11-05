// Pure parts of the toolpath preview: how fast the animation moves, putting
// the kernel's changed cells into the shown heightfield, and the colours the
// machined areas are painted.

import type { Heightfield, SimPatch, ToolpathSummary } from '../../bridge/cam';
import { copyHeights, heightReader, solidFloor } from '../../bridge/heights';
import type { MaterialTexture } from '../../bridge/prefs';
import type { Side } from '../../bridge/types';
import type { ModelColors } from '../../bridge/componentTree';

/** How the preview is painted: the material, the uncut surface in one
 * colour, the machined areas in one colour or a colour per toolpath. */
export type FillMode = 'material' | 'surface' | 'global' | 'toolpath';

/** The Machined Area Color radios: a fill mode, or the lithophane shading. */
export type AreaChoice = FillMode | 'lithophane';

/** The radio shown for a fill mode and the lithophane switch. */
export function areaChoice(fill: FillMode, lithophane: boolean): AreaChoice {
  return lithophane ? 'lithophane' : fill;
}

/** The fill mode and lithophane switch a radio picks; Lithophane shades the
 * material itself, so the machined areas keep the material color. */
export function fromAreaChoice(choice: AreaChoice): { fill: FillMode; lithophane: boolean } {
  return choice === 'lithophane'
    ? { fill: 'material', lithophane: true }
    : { fill: choice, lithophane: false };
}

/** The Toolpath Color swatch: the selected toolpath's color (or 'none'),
 * null when no toolpath is selected. */
export function selectedColour(
  toolpaths: readonly Pick<ToolpathSummary, 'id'>[],
  selectedId: number | null,
  chosen: Readonly<Record<number, string>>,
): string | null {
  const position = toolpaths.findIndex((t) => t.id === selectedId);
  if (selectedId === null || position < 0) {
    return null;
  }
  return toolpathColour(position, selectedId, chosen);
}

/** Slowest and fastest animation speed on the slider. */
export const SPEED_MIN = 1;
export const SPEED_MAX = 10;

/** Default colours given to toolpaths in list order. */
export const TOOLPATH_PALETTE: readonly string[] = [
  '#d9534f',
  '#3b82c4',
  '#4caf50',
  '#e6a23c',
  '#8e5bc4',
  '#1fa3a3',
  '#c45b95',
  '#7a8b2f',
];

/** The time between animation frames the animation aims for, ms. */
export const FRAME_MS = 1000 / 60;
/** The longest time one animation step covers, in frames: a step that
 * took long (a slow frame, a busy kernel) slows the tool down rather than
 * making it leap ahead. */
const MOST_FRAMES = 2;

/** Millimetres of toolpath the tool covers a second at a slider speed. */
export function animationRate(speed: number): number {
  const s = Math.min(SPEED_MAX, Math.max(SPEED_MIN, Math.round(speed)));
  // About 70 mm a second at the slowest to 800 m at the fastest.
  return 25 * 2 ** (1.5 * s);
}

/**
 * How far the tool moves in an animation step that follows the last one
 * by `elapsed` ms, so it moves at the same speed however often steps come.
 */
export function frameDistance(speed: number, elapsed: number): number {
  const ms = Math.min(Math.max(elapsed, 0), FRAME_MS * MOST_FRAMES);
  return (animationRate(speed) * ms) / 1000;
}

/** How long a step may keep the kernel busy, ms, so frames keep coming. */
const STEP_BUDGET_MS = FRAME_MS;
/** A step covers at least this share of a frame's worth at the chosen
 * speed, however slow the kernel is. */
const LEAST_SHARE = 0.25;

/**
 * The step distance for `elapsed` ms since the last step when steps have
 * been costing `msPerMm` (null before the first): as far as the speed
 * says, but no further than the kernel can cut within a frame, so a fine
 * preview quality slows the tool evenly instead of dropping frames.
 */
export function paceDistance(speed: number, elapsed: number, msPerMm: number | null): number {
  const wanted = frameDistance(speed, elapsed);
  if (msPerMm === null || !(msPerMm > 0)) {
    return wanted;
  }
  const least = ((animationRate(speed) * FRAME_MS) / 1000) * LEAST_SHARE;
  return Math.min(wanted, Math.max(STEP_BUDGET_MS / msPerMm, least));
}

/** The step cost after a step of `distance` mm took `ms`, smoothed. */
export function nextStepCost(msPerMm: number | null, ms: number, distance: number): number | null {
  if (!(distance > 1e-6) || !(ms >= 0)) {
    return msPerMm;
  }
  const sample = ms / distance;
  return msPerMm === null ? sample : msPerMm * 0.75 + sample * 0.25;
}

/** The colour a toolpath is painted with: its own choice or the palette's. */
export function toolpathColour(
  position: number,
  id: number,
  chosen: Readonly<Record<number, string>>,
): string {
  return chosen[id] ?? TOOLPATH_PALETTE[position % TOOLPATH_PALETTE.length] ?? '#d9534f';
}

/** How many cells a patch's rectangles hold. */
export function patchCells(patch: SimPatch): number {
  return patch.rects.reduce((sum, r) => sum + r.width * r.height, 0);
}

/** Whether a patch leaves a flat underside flat: it sends no underside, or
 * its floor is `bottom` wherever its top has material and NaN where it is
 * cut through. */
function keepsFlat(patch: SimPatch, bottom: number): boolean {
  const cells = patchCells(patch);
  if (patch.floor.data.length < cells) {
    return true;
  }
  const top = heightReader(patch.z);
  const under = heightReader(patch.floor);
  const slack = Math.max(patch.floor.code?.step ?? 0, 1e-4);
  for (let i = 0; i < cells; i += 1) {
    const f = under(i);
    if (Number.isNaN(top(i)) ? !Number.isNaN(f) : Math.abs(f - bottom) > slack) {
      return false;
    }
  }
  return true;
}

/**
 * Copies a patch of changed cells into the heightfield's arrays and answers
 * a new heightfield object over them, so views see the change. Heights go
 * in as codes when the patch shares the field's code. A flat underside the
 * patch raises becomes heights of its own. A patch that does not fit the
 * field is ignored.
 */
export function applyPatch(field: Heightfield, patch: SimPatch): Heightfield {
  const cells = patchCells(patch);
  if (
    cells === 0 ||
    patch.z.data.length < cells ||
    patch.rects.some(
      (r) => r.x0 < 0 || r.y0 < 0 || r.x0 + r.width > field.width || r.y0 + r.height > field.height,
    )
  ) {
    return field;
  }
  let floor = field.floor;
  if (floor === 'flat' && !keepsFlat(patch, field.bottom)) {
    floor = solidFloor(field);
  }
  const ownerToo = field.owner.length === field.z.data.length;
  const under =
    floor !== undefined && floor !== 'flat' && patch.floor.data.length >= cells ? floor : null;
  // The first laser marks make the field's own array of them.
  let burn = field.burn;
  const marks = patch.burn !== undefined && patch.burn.length >= cells ? patch.burn : null;
  if (marks !== null && burn?.length !== field.width * field.height) {
    burn = new Uint8Array(field.width * field.height);
  }
  let from = 0;
  for (const { x0, y0, width, height } of patch.rects) {
    for (let j = 0; j < height; j += 1) {
      const to = (y0 + j) * field.width + x0;
      copyHeights(field.z, to, patch.z, from, width);
      if (ownerToo) {
        field.owner.set(patch.owner.subarray(from, from + width), to);
      }
      if (under !== null) {
        copyHeights(under, to, patch.floor, from, width);
      }
      if (marks !== null && burn !== undefined) {
        burn.set(marks.subarray(from, from + width), to);
      }
      from += width;
    }
  }
  const marked = burn === undefined ? {} : { burn };
  return floor === undefined ? { ...field, ...marked } : { ...field, floor, ...marked };
}

/** `#rrggbb` as bytes, or null when it is not one. */
function rgb(colour: string): [number, number, number] | null {
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
 * Colours for the machined cells of a preview (cells a toolpath cut) in the
 * shape the 3D view paints components with; null for the plain material.
 * `colourOf` answers the colour of a toolpath by its owner tag (list
 * position plus one), or null for no fill.
 */
export function machinedColours(
  field: Pick<Heightfield, 'width' | 'height' | 'owner'>,
  mode: FillMode,
  global: string,
  colourOf: (owner: number) => string | null,
): ModelColors | null {
  if (mode === 'material' || field.owner.length !== field.width * field.height) {
    return null;
  }
  const rgba = new Uint8Array(field.width * field.height * 4);
  const one = rgb(global);
  const cache = new Map<number, [number, number, number] | null>();
  let any = false;
  for (let i = 0; i < field.owner.length; i += 1) {
    const owner = field.owner[i] ?? 0;
    // Surface Color paints the uncut cells alone; the others the cut ones.
    if ((owner === 0) !== (mode === 'surface')) {
      continue;
    }
    let colour = one;
    if (mode === 'toolpath') {
      if (!cache.has(owner)) {
        const chosen = colourOf(owner);
        cache.set(owner, chosen === null ? null : rgb(chosen));
      }
      colour = cache.get(owner) ?? null;
    }
    if (colour === null) {
      continue;
    }
    rgba[i * 4] = colour[0];
    rgba[i * 4 + 1] = colour[1];
    rgba[i * 4 + 2] = colour[2];
    rgba[i * 4 + 3] = 255;
    any = true;
  }
  return any ? { width: field.width, height: field.height, rgba, any } : null;
}

/**
 * The visible toolpaths a double sided job previews: those of the side
 * shown, or of both sides for Preview All Sides and Multi Sided View.
 */
export function visibleOnSides(
  toolpaths: readonly Pick<ToolpathSummary, 'id' | 'visible' | 'side'>[],
  side: Side,
  allSides: boolean,
): number[] {
  return toolpaths
    .filter((t) => t.visible && (allSides || (t.side ?? 'top') === side))
    .map((t) => t.id);
}

/** The value the material list uses for a texture picture. */
export function textureValue(id: number): string {
  return `texture:${id}`;
}

/** The texture id a material list value names, or null for a plain material. */
export function textureOf(value: string): number | null {
  const match = /^texture:(\d+)$/.exec(value);
  return match === null ? null : Number(match[1]);
}

/** The material textures grouped by category, categories in first use order. */
export function texturesByCategory(
  textures: readonly MaterialTexture[],
): Array<{ category: string; textures: MaterialTexture[] }> {
  const groups: Array<{ category: string; textures: MaterialTexture[] }> = [];
  for (const texture of textures) {
    const group = groups.find((g) => g.category === texture.category);
    if (group === undefined) {
      groups.push({ category: texture.category, textures: [texture] });
    } else {
      group.textures.push(texture);
    }
  }
  return groups;
}

/** The material list value that adds a texture under `category`. */
export function addTextureValue(category: string): string {
  return `add-texture:${category}`;
}

/** The category an add texture list value names, or null for another value. */
export function addTextureCategory(value: string): string | null {
  return value.startsWith('add-texture:') ? value.slice('add-texture:'.length) : null;
}

/** A typed category name, tidied, or null when there is none. */
export function categoryName(typed: string | null): string | null {
  const name = typed?.trim() ?? '';
  return name === '' ? null : name;
}

/** The texture list with a picture file added under `category`, named after the file. */
export function addTexture(
  textures: readonly MaterialTexture[],
  path: string,
  category: string,
): { textures: MaterialTexture[]; id: number } {
  const id = textures.reduce((most, t) => Math.max(most, t.id), 0) + 1;
  const file = path.split(/[\\/]/).pop() ?? path;
  const name = file.replace(/\.[^.]+$/, '') || file;
  return { textures: [...textures, { id, name, category, path }], id };
}
