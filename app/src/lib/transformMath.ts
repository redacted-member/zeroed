// Matrices for the Transform Objects forms: move, set size, rotate, mirror,
// align and distribute, all expressed on bounding boxes in job millimetres.

import type { Bbox, Matrix, Point, SelectionFrame, SelectionState } from '../bridge/document';
import { applyMatrix, multiply, rotationAbout, scaleAbout, translation } from './geometry';
import { boxOrigin, type Anchor } from './shapes';

export interface Placed {
  id: number;
  bbox: Bbox;
}

export interface PlacedMatrix {
  id: number;
  matrix: Matrix;
}

export function bboxWidth(bbox: Bbox): number {
  return bbox[2] - bbox[0];
}

export function bboxHeight(bbox: Bbox): number {
  return bbox[3] - bbox[1];
}

export function bboxCentre(bbox: Bbox): Point {
  return { x: (bbox[0] + bbox[2]) / 2, y: (bbox[1] + bbox[3]) / 2 };
}

/** The anchor point of a bounding box. */
export function bboxAnchor(bbox: Bbox, anchor: Anchor): Point {
  const shifted = boxOrigin(anchor, 0, 0, bboxWidth(bbox), bboxHeight(bbox));
  return { x: bbox[0] - shifted.x, y: bbox[1] - shifted.y };
}

export function unionOf(boxes: readonly Bbox[]): Bbox | null {
  let result: Bbox | null = null;
  for (const box of boxes) {
    result =
      result === null
        ? [...box]
        : [
            Math.min(result[0], box[0]),
            Math.min(result[1], box[1]),
            Math.max(result[2], box[2]),
            Math.max(result[3], box[3]),
          ];
  }
  return result;
}

export type MoveMode = 'absolute' | 'relative';

/** Moves the selection so its anchor lands on (x, y), or by (x, y). */
export function moveMatrix(
  bbox: Bbox,
  anchor: Anchor,
  mode: MoveMode,
  x: number,
  y: number,
): Matrix {
  if (mode === 'relative') {
    return translation(x, y);
  }
  const at = bboxAnchor(bbox, anchor);
  return translation(x - at.x, y - at.y);
}

/** Scales the selection to the given size about its anchor. */
export function setSizeMatrix(bbox: Bbox, anchor: Anchor, width: number, height: number): Matrix {
  const at = bboxAnchor(bbox, anchor);
  const currentWidth = bboxWidth(bbox);
  const currentHeight = bboxHeight(bbox);
  const sx = currentWidth > 0 ? width / currentWidth : 1;
  const sy = currentHeight > 0 ? height / currentHeight : 1;
  return scaleAbout(sx, sy, at.x, at.y);
}

/**
 * The frame a transform form works in: the selection's rotated frame when it
 * has one, else its plain bounding box with angle 0.
 */
export function selectionFrame(selection: SelectionState): SelectionFrame | null {
  if (selection.bbox === null) {
    return null;
  }
  return selection.frame ?? { angle: 0, extent: selection.bbox };
}

/** A frame space point in job space. */
export function fromFrame(frame: SelectionFrame, p: Point): Point {
  return frame.angle === 0 ? p : applyMatrix(rotationAbout(frame.angle, 0, 0), p);
}

/** A job space point in frame space. */
export function toFrame(frame: SelectionFrame, p: Point): Point {
  return frame.angle === 0 ? p : applyMatrix(rotationAbout(-frame.angle, 0, 0), p);
}

/**
 * A pivot placed by clicking: it snaps to a corner, an edge middle or the
 * centre of the selection's box when within `tolerance` of one.
 */
export function snapPivot(frame: SelectionFrame | null, at: Point, tolerance: number): Point {
  if (frame === null) {
    return at;
  }
  const [x0, y0, x1, y1] = frame.extent;
  const xs = [x0, (x0 + x1) / 2, x1];
  const ys = [y0, (y0 + y1) / 2, y1];
  let best: { p: Point; d: number } | null = null;
  for (const x of xs) {
    for (const y of ys) {
      const p = fromFrame(frame, { x, y });
      const d = Math.hypot(p.x - at.x, p.y - at.y);
      if (d <= tolerance && (best === null || d < best.d)) {
        best = { p, d };
      }
    }
  }
  return best?.p ?? at;
}

/** A frame space transform expressed in job space. */
export function inFrame(frame: SelectionFrame, m: Matrix): Matrix {
  if (frame.angle === 0) {
    return m;
  }
  return compose(rotationAbout(-frame.angle, 0, 0), m, rotationAbout(frame.angle, 0, 0));
}

/** The anchor point of the frame's box, in job space. */
export function frameAnchor(frame: SelectionFrame, anchor: Anchor): Point {
  return fromFrame(frame, bboxAnchor(frame.extent, anchor));
}

/** The frame box corners in job space: bottom left, bottom right, top right, top left. */
export function frameCorners(frame: SelectionFrame): Point[] {
  const [minX, minY, maxX, maxY] = frame.extent;
  return [
    { x: minX, y: minY },
    { x: maxX, y: minY },
    { x: maxX, y: maxY },
    { x: minX, y: maxY },
  ].map((p) => fromFrame(frame, p));
}

/** Scales a selection to the given size along its own frame, about the anchor. */
export function setFrameSizeMatrix(
  frame: SelectionFrame,
  anchor: Anchor,
  width: number,
  height: number,
): Matrix {
  return inFrame(frame, setSizeMatrix(frame.extent, anchor, width, height));
}

/** Rotates about a pivot, counter clockwise for positive angles. */
export function rotateMatrix(pivot: Point, angleDeg: number): Matrix {
  return rotationAbout(angleDeg, pivot.x, pivot.y);
}

export type MirrorMode = 'horizontal' | 'vertical' | 'left' | 'right' | 'top' | 'bottom';

/** Reflection across the line through `a` and `b` (identity when they coincide). */
export function mirrorAcrossLine(a: Point, b: Point): Matrix {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const length = Math.hypot(dx, dy);
  if (length < 1e-12) {
    return [1, 0, 0, 1, 0, 0];
  }
  const angle = (Math.atan2(dy, dx) * 180) / Math.PI;
  return compose(
    translation(-a.x, -a.y),
    rotationAbout(-angle, 0, 0),
    scaleAbout(1, -1, 0, 0),
    rotationAbout(angle, 0, 0),
    translation(a.x, a.y),
  );
}

/**
 * Mirrors the selection: horizontal and vertical flip in place about the
 * selection centre; the edge modes flip across that edge so the result
 * sits beside the original.
 */
export function mirrorMatrix(bbox: Bbox, mode: MirrorMode): Matrix {
  const centre = bboxCentre(bbox);
  switch (mode) {
    case 'horizontal':
      return scaleAbout(-1, 1, centre.x, centre.y);
    case 'vertical':
      return scaleAbout(1, -1, centre.x, centre.y);
    case 'left':
      return scaleAbout(-1, 1, bbox[0], centre.y);
    case 'right':
      return scaleAbout(-1, 1, bbox[2], centre.y);
    case 'top':
      return scaleAbout(1, -1, centre.x, bbox[3]);
    case 'bottom':
      return scaleAbout(1, -1, centre.x, bbox[1]);
  }
}

export type AlignMode =
  | 'left'
  | 'right'
  | 'top'
  | 'bottom'
  | 'hCentre'
  | 'vCentre'
  | 'centre'
  | 'outsideLeft'
  | 'outsideRight'
  | 'outsideTop'
  | 'outsideBottom';

/** Translations that align each item to the target box. */
export function alignMatrices(
  items: readonly Placed[],
  target: Bbox,
  mode: AlignMode,
): PlacedMatrix[] {
  const targetCentre = bboxCentre(target);
  return items.map((item) => {
    const centre = bboxCentre(item.bbox);
    let dx = 0;
    let dy = 0;
    switch (mode) {
      case 'left':
        dx = target[0] - item.bbox[0];
        break;
      case 'right':
        dx = target[2] - item.bbox[2];
        break;
      case 'top':
        dy = target[3] - item.bbox[3];
        break;
      case 'bottom':
        dy = target[1] - item.bbox[1];
        break;
      case 'hCentre':
        dx = targetCentre.x - centre.x;
        break;
      case 'vCentre':
        dy = targetCentre.y - centre.y;
        break;
      case 'centre':
        dx = targetCentre.x - centre.x;
        dy = targetCentre.y - centre.y;
        break;
      case 'outsideLeft':
        dx = target[0] - item.bbox[2];
        break;
      case 'outsideRight':
        dx = target[2] - item.bbox[0];
        break;
      case 'outsideTop':
        dy = target[3] - item.bbox[1];
        break;
      case 'outsideBottom':
        dy = target[1] - item.bbox[3];
        break;
    }
    return { id: item.id, matrix: translation(dx, dy) };
  });
}

export type DistributeMode =
  'hLeft' | 'hCentre' | 'hRight' | 'hGap' | 'vTop' | 'vCentre' | 'vBottom' | 'vGap';

/**
 * Translations that spread the items evenly. The outermost items stay put;
 * with `spacing` the items are laid out from the first one with that gap.
 */
export function distributeMatrices(
  items: readonly Placed[],
  mode: DistributeMode,
  spacing: number | null = null,
): PlacedMatrix[] {
  if (items.length < 2) {
    return items.map((item) => ({ id: item.id, matrix: [1, 0, 0, 1, 0, 0] }));
  }
  const horizontal = mode.startsWith('h');
  const key = (bbox: Bbox): number => {
    switch (mode) {
      case 'hLeft':
        return bbox[0];
      case 'hRight':
        return bbox[2];
      case 'hCentre':
      case 'hGap':
        return (bbox[0] + bbox[2]) / 2;
      case 'vBottom':
        return bbox[1];
      case 'vTop':
        return bbox[3];
      case 'vCentre':
      case 'vGap':
        return (bbox[1] + bbox[3]) / 2;
    }
  };
  const sorted = [...items].sort((a, b) => key(a.bbox) - key(b.bbox));
  const size = (bbox: Bbox) => (horizontal ? bboxWidth(bbox) : bboxHeight(bbox));
  const low = (bbox: Bbox) => (horizontal ? bbox[0] : bbox[1]);
  const result = new Map<number, number>();

  if (mode === 'hGap' || mode === 'vGap') {
    const first = sorted[0];
    const last = sorted[sorted.length - 1];
    if (first === undefined || last === undefined) {
      return [];
    }
    let gap = spacing;
    if (gap === null) {
      const total = sorted.reduce((sum, item) => sum + size(item.bbox), 0);
      const span = low(last.bbox) + size(last.bbox) - low(first.bbox);
      gap = (span - total) / (sorted.length - 1);
    }
    let cursor = low(first.bbox);
    for (const item of sorted) {
      result.set(item.id, cursor - low(item.bbox));
      cursor += size(item.bbox) + gap;
    }
  } else {
    const first = sorted[0];
    const last = sorted[sorted.length - 1];
    if (first === undefined || last === undefined) {
      return [];
    }
    const start = key(first.bbox);
    const step =
      spacing !== null ? spacing : (key(last.bbox) - key(first.bbox)) / (sorted.length - 1);
    sorted.forEach((item, index) => {
      result.set(item.id, start + step * index - key(item.bbox));
    });
  }
  return items.map((item) => {
    const delta = result.get(item.id) ?? 0;
    return { id: item.id, matrix: horizontal ? translation(delta, 0) : translation(0, delta) };
  });
}

/** Composes a list of matrices, first applied first. */
export function compose(...matrices: Matrix[]): Matrix {
  return matrices.reduce<Matrix>((acc, m) => multiply(m, acc), [1, 0, 0, 1, 0, 0]);
}
