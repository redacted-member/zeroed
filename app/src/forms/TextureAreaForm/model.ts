// Pure state for the Create Texture Area form: the tile component and the
// optional boundary, spacing, row and column shifts, the reflection of each
// tile of a group of four, and the tile box the Edit Textured Area
// Component mode moves and sizes.

import type { EntitySummary, Path, Point } from '../../bridge/document';
import type {
  Combine,
  ComponentSummary,
  Reflect,
  TextureParams,
  TileBox,
} from '../../bridge/model';
import { fieldError, numberValue } from '../../lib/formFields';
import { applyMatrix, rectPath, rotationAbout, transformPath } from '../../lib/geometry';

/** Transform Object moves and sizes the whole area; Edit Textured Area
 * Component moves and sizes the tile. */
export type TextureMode = 'transform' | 'editTile';

export interface TextureFormState {
  name: string;
  combine: Combine;
  mode: TextureMode;
  /** Percent of the tile width. */
  spacingX: string;
  /** Percent of the tile height. */
  spacingY: string;
  /** Percent of the tile width, odd rows. */
  shiftX: string;
  /** Percent of the tile height, odd columns. */
  shiftY: string;
  /** Lower left, lower right, upper left, upper right. */
  reflect: [Reflect, Reflect, Reflect, Reflect];
  /** The tile box, known after the first Apply or when editing. */
  tile: TileBox | null;
  /** Turn of the tile lattice about the tile's corner, degrees; a turned
   * texture area keeps it. */
  angle: number;
}

export type TextureField = 'spacingX' | 'spacingY' | 'shiftX' | 'shiftY';
export type TextureErrors = Partial<Record<TextureField, string>>;

export const REFLECT_ORDER: readonly Reflect[] = ['none', 'horizontal', 'vertical', 'both'];

export function initialTextureState(): TextureFormState {
  return {
    name: '',
    combine: 'add',
    mode: 'transform',
    spacingX: '0',
    spacingY: '0',
    shiftX: '0',
    shiftY: '0',
    reflect: ['none', 'none', 'none', 'none'],
    tile: null,
    angle: 0,
  };
}

function text(value: number): string {
  return String(Math.round(value * 100) / 100);
}

/** The form as a texture area component was last applied. */
export function stateFromTexture(component: ComponentSummary): TextureFormState {
  const initial = initialTextureState();
  const params = component.texture?.params;
  if (params === undefined) {
    return initial;
  }
  return {
    ...initial,
    name: component.name,
    combine: component.combine,
    spacingX: text(params.spacingX),
    spacingY: text(params.spacingY),
    shiftX: text(params.shiftX),
    shiftY: text(params.shiftY),
    reflect: [...params.reflect],
    tile: params.tile ?? null,
    angle: params.angle ?? 0,
  };
}

/** The next reflection of one tile: none, horizontal, vertical, both. */
export function nextReflect(state: TextureFormState, quarter: 0 | 1 | 2 | 3): TextureFormState {
  const reflect: [Reflect, Reflect, Reflect, Reflect] = [...state.reflect];
  const now = REFLECT_ORDER.indexOf(reflect[quarter]);
  reflect[quarter] = REFLECT_ORDER[(now + 1) % REFLECT_ORDER.length] ?? 'none';
  return { ...state, reflect };
}

export function validateTexture(state: TextureFormState): TextureErrors {
  const errors: TextureErrors = {};
  const checks: Array<[TextureField, string | undefined]> = [
    ['spacingX', fieldError(numberValue(state.spacingX), { min: -89 })],
    ['spacingY', fieldError(numberValue(state.spacingY), { min: -89 })],
    ['shiftX', fieldError(numberValue(state.shiftX))],
    ['shiftY', fieldError(numberValue(state.shiftY))],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

export function textureParams(state: TextureFormState): TextureParams | null {
  if (Object.keys(validateTexture(state)).length > 0) {
    return null;
  }
  return {
    spacingX: numberValue(state.spacingX) ?? 0,
    spacingY: numberValue(state.spacingY) ?? 0,
    shiftX: numberValue(state.shiftX) ?? 0,
    shiftY: numberValue(state.shiftY) ?? 0,
    reflect: [...state.reflect],
    tile: state.tile,
    angle: state.angle,
  };
}

/** The closed vectors among the selection: the area's boundary. */
export function boundaryFromSelection(
  ids: readonly number[],
  entities: readonly EntitySummary[],
): number[] {
  const closed = new Set(entities.filter((e) => e.kind === 'path' && e.closed).map((e) => e.id));
  return ids.filter((id) => closed.has(id));
}

/** The component to tile: the one picked, unless it is itself a texture
 * area (then its own seed). */
export function seedOf(component: ComponentSummary | null): number | null {
  if (component === null) {
    return null;
  }
  return component.texture?.seed ?? component.id;
}

/** Which handle of the tile box is under `p`: a corner (to size it from
 * the opposite corner) or the inside (to move it). */
export type TileGrip = { kind: 'corner'; anchor: Point } | { kind: 'move'; from: Point };

/** `p` in the frame of a lattice turned by `angle` degrees about the
 * tile's corner, where the tile box is square to the axes. */
export function toLattice(tile: TileBox, angle: number, p: Point): Point {
  return angle === 0 ? p : applyMatrix(rotationAbout(-angle, tile.x, tile.y), p);
}

/** The outline of the tile box as it lies on the job, turned with the lattice. */
export function tileOutline(tile: TileBox, angle: number): Path {
  const box = rectPath(tile.x, tile.y, tile.width, tile.height);
  return angle === 0 ? box : transformPath(box, rotationAbout(angle, tile.x, tile.y));
}

export function tileGripAt(
  tile: TileBox,
  job: Point,
  tolerance: number,
  angle = 0,
): TileGrip | null {
  const p = toLattice(tile, angle, job);
  const corners: Array<[Point, Point]> = [
    [
      { x: tile.x, y: tile.y },
      { x: tile.x + tile.width, y: tile.y + tile.height },
    ],
    [
      { x: tile.x + tile.width, y: tile.y },
      { x: tile.x, y: tile.y + tile.height },
    ],
    [
      { x: tile.x + tile.width, y: tile.y + tile.height },
      { x: tile.x, y: tile.y },
    ],
    [
      { x: tile.x, y: tile.y + tile.height },
      { x: tile.x + tile.width, y: tile.y },
    ],
  ];
  for (const [corner, anchor] of corners) {
    if (Math.hypot(corner.x - p.x, corner.y - p.y) <= tolerance) {
      return { kind: 'corner', anchor };
    }
  }
  const inside =
    p.x >= tile.x && p.x <= tile.x + tile.width && p.y >= tile.y && p.y <= tile.y + tile.height;
  return inside ? { kind: 'move', from: job } : null;
}

/** The tile box after dragging `grip` from where it started to `p`. */
export function dragTile(tile: TileBox, grip: TileGrip, job: Point, angle = 0): TileBox {
  if (grip.kind === 'move') {
    return { ...tile, x: tile.x + job.x - grip.from.x, y: tile.y + job.y - grip.from.y };
  }
  // Sized square to the lattice, then its corner taken back onto the job.
  const p = toLattice(tile, angle, job);
  const min = 0.1;
  const x0 = Math.min(grip.anchor.x, p.x);
  const y0 = Math.min(grip.anchor.y, p.y);
  const corner =
    angle === 0
      ? { x: x0, y: y0 }
      : applyMatrix(rotationAbout(angle, tile.x, tile.y), { x: x0, y: y0 });
  return {
    x: corner.x,
    y: corner.y,
    width: Math.max(min, Math.abs(p.x - grip.anchor.x)),
    height: Math.max(min, Math.abs(p.y - grip.anchor.y)),
  };
}
