// Which transform handles the selection shows after a pick. In Vector
// Selection mode a pick only selects and a second click on the selection
// brings up the handles; in Interactive Selection mode every pick shows them
// at once. White corners and edges scale, the solid corners past them rotate
// about the rotation center. Clicking the selection again shows the
// rotational anchor, which drags to move that center; another click hides it.

import type { Matrix, Point, SelectionFrame } from '../../../bridge/document';
import { applyMatrix } from '../../../lib/geometry';
import { fromFrame } from '../../../lib/transformMath';

/** No handles, the scale and rotate handles, or those plus the rotational anchor. */
export type HandleMode = 'none' | 'scale' | 'pivot';

/** Handles after a pick that selects new objects (click, band or component). */
export function handlesAfterPick(transformMode: boolean): HandleMode {
  return transformMode ? 'scale' : 'none';
}

/** Handles after a plain click on an object that was already selected. */
export function handlesAfterReclick(current: HandleMode): HandleMode {
  return current === 'scale' ? 'pivot' : 'scale';
}

/** Handles after a drag moved the selection. */
export function handlesAfterMove(transformMode: boolean, current: HandleMode): HandleMode {
  return transformMode && current === 'none' ? 'scale' : current;
}

/** Which handle a scale drag holds: a corner or the middle of an edge. */
export type ScaleHandleKind = 'corner' | 'horizontalEdge' | 'verticalEdge';

/**
 * Scale factors for a handle drag from the raw factors along each axis. A
 * corner scales in proportion unless Alt is held; an edge scales its own
 * axis only unless Shift is held, when the other axis follows.
 */
export function scaleFactors(
  kind: ScaleHandleKind,
  rawX: number,
  rawY: number,
  keys: { alt: boolean; shift: boolean },
): { sx: number; sy: number } {
  const uniform = (value: number, sign: number) => Math.sign(sign || 1) * Math.abs(value);
  if (kind === 'corner') {
    if (keys.alt) {
      return { sx: rawX, sy: rawY };
    }
    const size = Math.max(Math.abs(rawX), Math.abs(rawY));
    return { sx: uniform(size, rawX), sy: uniform(size, rawY) };
  }
  if (kind === 'horizontalEdge') {
    return { sx: rawX, sy: keys.shift ? Math.abs(rawX) : 1 };
  }
  return { sx: keys.shift ? Math.abs(rawY) : 1, sy: rawY };
}

/** Rotation snapped to 15 degree steps while Alt is held. */
export function rotationStep(degrees: number, alt: boolean): number {
  return alt ? Math.round(degrees / 15) * 15 : degrees;
}

/**
 * The point the rotate handles turn the selection about: the moved rotation
 * center, or the middle of the selection's box while it has not been moved.
 */
export function rotationCentre(frame: SelectionFrame, moved: Point | null): Point {
  if (moved !== null) {
    return moved;
  }
  const [minX, minY, maxX, maxY] = frame.extent;
  return fromFrame(frame, { x: (minX + maxX) / 2, y: (minY + maxY) / 2 });
}

/** A moved rotation center carried along by a move or scale of the selection. */
export function centreAfterTransform(moved: Point | null, matrix: Matrix): Point | null {
  return moved === null ? null : applyMatrix(matrix, moved);
}
