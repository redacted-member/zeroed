// Transform handles for a component in the 3D view: the middle moves it,
// the corners scale it (proportionally, or freely with Alt; about the
// opposite corner, or the middle with Shift), the edge handles stretch it
// along one axis (both with Shift) and the arrow above the top edge turns
// it (in 15 degree steps with Alt). Everything here is job XY millimetres;
// the view projects the points to the screen.

import type { ComponentTransformParams } from '../../bridge/model';

export type Bbox = readonly [number, number, number, number];

export interface XY {
  x: number;
  y: number;
}

/** A handle: the middle, a corner or edge (0 to 3, counter clockwise from
 * the bottom left corner or bottom edge) or the rotation arrow. */
export type HandleKind =
  | { kind: 'move' }
  | { kind: 'corner'; index: 0 | 1 | 2 | 3 }
  | { kind: 'edge'; index: 0 | 1 | 2 | 3 }
  | { kind: 'rotate' };

/** Keys held while dragging. */
export interface DragKeys {
  alt: boolean;
  shift: boolean;
}

/** How far above the top edge the rotation arrow sits, as a fraction of
 * the box height (at least a little for a flat box). */
const ROTATE_REACH = 0.18;
const MIN_SCALE = 0.01;
const ROTATE_STEP = 15;

export const CORNERS = [0, 1, 2, 3] as const;

export function centreOf(bbox: Bbox): XY {
  return { x: (bbox[0] + bbox[2]) / 2, y: (bbox[1] + bbox[3]) / 2 };
}

export function cornerOf(bbox: Bbox, index: 0 | 1 | 2 | 3): XY {
  const [minX, minY, maxX, maxY] = bbox;
  return [
    { x: minX, y: minY },
    { x: maxX, y: minY },
    { x: maxX, y: maxY },
    { x: minX, y: maxY },
  ][index] as XY;
}

export function edgeOf(bbox: Bbox, index: 0 | 1 | 2 | 3): XY {
  const a = cornerOf(bbox, index);
  const b = cornerOf(bbox, ((index + 1) % 4) as 0 | 1 | 2 | 3);
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
}

export function rotateHandleOf(bbox: Bbox): XY {
  const height = bbox[3] - bbox[1];
  const width = bbox[2] - bbox[0];
  const reach = Math.max(height * ROTATE_REACH, Math.max(width, height) * 0.08, 1);
  return { x: (bbox[0] + bbox[2]) / 2, y: bbox[3] + reach };
}

/** The job point of a handle on an untransformed box. */
export function handlePoint(bbox: Bbox, handle: HandleKind): XY {
  switch (handle.kind) {
    case 'move':
      return centreOf(bbox);
    case 'corner':
      return cornerOf(bbox, handle.index);
    case 'edge':
      return edgeOf(bbox, handle.index);
    case 'rotate':
      return rotateHandleOf(bbox);
  }
}

function positive(value: number): number {
  return Number.isFinite(value) ? Math.max(MIN_SCALE, value) : 1;
}

/** Ratio of `p` to `c` measured from `a` along one axis. */
function along(p: number, c: number, a: number): number {
  const span = c - a;
  return Math.abs(span) < 1e-9 ? 1 : (p - a) / span;
}

const IDENTITY: ComponentTransformParams = {
  offset: { x: 0, y: 0 },
  scaleX: 1,
  scaleY: 1,
  rotateDeg: 0,
};

/** The transform a drag from `start` to `now` on `handle` asks for; the
 * rotation turns about `pivot`, or the middle of the box when null. */
export function dragParams(
  handle: HandleKind,
  bbox: Bbox,
  start: XY,
  now: XY,
  keys: DragKeys,
  pivot: XY | null = null,
): ComponentTransformParams {
  const centre = centreOf(bbox);
  switch (handle.kind) {
    case 'move': {
      let dx = now.x - start.x;
      let dy = now.y - start.y;
      if (keys.alt) {
        if (Math.abs(dx) >= Math.abs(dy)) {
          dy = 0;
        } else {
          dx = 0;
        }
      }
      return { ...IDENTITY, offset: { x: dx, y: dy } };
    }
    case 'corner': {
      const c = cornerOf(bbox, handle.index);
      const a = keys.shift ? centre : cornerOf(bbox, ((handle.index + 2) % 4) as 0 | 1 | 2 | 3);
      if (keys.alt) {
        return {
          ...IDENTITY,
          scaleX: positive(along(now.x, c.x, a.x)),
          scaleY: positive(along(now.y, c.y, a.y)),
          centre: a,
        };
      }
      const vx = c.x - a.x;
      const vy = c.y - a.y;
      const length = vx * vx + vy * vy;
      const s = length < 1e-12 ? 1 : ((now.x - a.x) * vx + (now.y - a.y) * vy) / length;
      return { ...IDENTITY, scaleX: positive(s), scaleY: positive(s), centre: a };
    }
    case 'edge': {
      const e = edgeOf(bbox, handle.index);
      const a = edgeOf(bbox, ((handle.index + 2) % 4) as 0 | 1 | 2 | 3);
      const horizontal = handle.index === 1 || handle.index === 3;
      const s = positive(horizontal ? along(now.x, e.x, a.x) : along(now.y, e.y, a.y));
      if (keys.shift) {
        return { ...IDENTITY, scaleX: s, scaleY: s, centre: a };
      }
      return horizontal
        ? { ...IDENTITY, scaleX: s, centre: a }
        : { ...IDENTITY, scaleY: s, centre: a };
    }
    case 'rotate': {
      const about = pivot ?? centre;
      const from = Math.atan2(start.y - about.y, start.x - about.x);
      const to = Math.atan2(now.y - about.y, now.x - about.x);
      let deg = ((to - from) * 180) / Math.PI;
      deg = ((((deg + 180) % 360) + 360) % 360) - 180;
      if (keys.alt) {
        deg = Math.round(deg / ROTATE_STEP) * ROTATE_STEP;
      }
      return { ...IDENTITY, rotateDeg: deg, centre: about };
    }
  }
}

/** Where `p` lands under `params` (scale, then turn, about the centre, then
 * the move), as the kernel applies it. */
export function applyParams(bbox: Bbox, params: ComponentTransformParams, p: XY): XY {
  const c = params.centre ?? centreOf(bbox);
  const sx = (p.x - c.x) * params.scaleX;
  const sy = (p.y - c.y) * params.scaleY;
  const r = (params.rotateDeg * Math.PI) / 180;
  const cos = Math.cos(r);
  const sin = Math.sin(r);
  return {
    x: c.x + sx * cos - sy * sin + params.offset.x,
    y: c.y + sx * sin + sy * cos + params.offset.y,
  };
}

/** Whether a transform changes anything. */
export function isIdentity(params: ComponentTransformParams): boolean {
  return (
    Math.abs(params.offset.x) < 1e-6 &&
    Math.abs(params.offset.y) < 1e-6 &&
    Math.abs(params.scaleX - 1) < 1e-6 &&
    Math.abs(params.scaleY - 1) < 1e-6 &&
    Math.abs(params.rotateDeg) < 1e-6
  );
}

/** Every handle, in drawing order. */
export const HANDLES: readonly HandleKind[] = [
  ...CORNERS.map((index) => ({ kind: 'edge', index }) as const),
  ...CORNERS.map((index) => ({ kind: 'corner', index }) as const),
  { kind: 'rotate' },
  { kind: 'move' },
];

/** A stable key for any handle of the view. */
export function handleKey(handle: { kind: string; index?: number }): string {
  return handle.index === undefined ? handle.kind : `${handle.kind}${handle.index}`;
}
