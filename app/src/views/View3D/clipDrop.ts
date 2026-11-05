// Where a clip dragged from the Clipart tab lands in the 3D view: on the
// model surface under the pointer, or where the pointer meets the top of
// the material when it points past the model.

import type { Point2 } from '../View2D/camera';

/** A point the view's picking answers, job X and Y. */
interface Picked {
  x: number;
  y: number;
}

/** The two picks the renderer offers, in normalised device coordinates. */
export interface ClipPicker {
  pick: (ndcX: number, ndcY: number) => Picked | null;
  pickPlane: (ndcX: number, ndcY: number, sceneY: number) => Picked | null;
}

/** Normalised device coordinates (-1 to 1 across and up) of a screen point
 * over `rect`. */
export function ndcIn(
  rect: { left: number; top: number; width: number; height: number },
  clientX: number,
  clientY: number,
): Point2 {
  return {
    x: ((clientX - rect.left) / rect.width) * 2 - 1,
    y: -(((clientY - rect.top) / rect.height) * 2 - 1),
  };
}

/** The job point a clip dropped at a screen point lands on, or null when
 * the pointer meets neither the model nor the material's top. */
export function clipDropPoint(
  picker: ClipPicker,
  rect: { left: number; top: number; width: number; height: number },
  clientX: number,
  clientY: number,
): Point2 | null {
  if (rect.width <= 0 || rect.height <= 0) {
    return null;
  }
  const ndc = ndcIn(rect, clientX, clientY);
  const hit = picker.pick(ndc.x, ndc.y) ?? picker.pickPlane(ndc.x, ndc.y, 0);
  return hit === null ? null : { x: hit.x, y: hit.y };
}
