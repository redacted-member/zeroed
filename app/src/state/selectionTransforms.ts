// Tells the 2D view when the whole selection moved, turned, scaled or was
// copied by a matrix, from a drag, a form, a menu or the keyboard, so what
// belongs to the selection in the view (the rotational anchor) goes with it.

import type { Matrix } from '../bridge/document';

type Listener = (matrix: Matrix) => void;

const listeners = new Set<Listener>();

/** Calls `listener` with the matrix of every whole selection transform; answers the unsubscribe. */
export function onSelectionTransformed(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The selection (or its copy, which becomes the selection) went by `matrix`. */
export function selectionTransformed(matrix: Matrix): void {
  for (const listener of listeners) {
    listener(matrix);
  }
}
