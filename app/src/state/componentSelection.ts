// Components selected in the 2D view by their previews, alongside the
// vectors: a click on a preview picks its component (Shift or Ctrl adds or
// takes it away), the picked components' box joins the selection's, and
// transforms of the selection move them too.

import { api } from '../bridge/api';
import type { Bbox, Matrix } from '../bridge/document';
import type { ComponentSummary } from '../bridge/model';
import { setSelectionExtras, useDocumentStore } from './documentStore';
import { pickedComponents, useModelStore } from './modelStore';

/** The topmost visible component whose box holds the point. */
export function componentUnder(
  components: readonly ComponentSummary[],
  x: number,
  y: number,
): ComponentSummary | null {
  for (let i = components.length - 1; i >= 0; i -= 1) {
    const c = components[i];
    if (c === undefined || !c.visible || c.bbox === undefined) {
      continue;
    }
    const [minX, minY, maxX, maxY] = c.bbox;
    if (x >= minX && x <= maxX && y >= minY && y <= maxY) {
      return c;
    }
  }
  return null;
}

/** The box round the components `ids`, or null for none. */
export function boxOf(
  components: readonly ComponentSummary[],
  ids: readonly number[],
): Bbox | null {
  let box: Bbox | null = null;
  for (const c of components) {
    if (!ids.includes(c.id) || c.bbox === undefined) {
      continue;
    }
    const b = c.bbox;
    box =
      box === null
        ? [b[0], b[1], b[2], b[3]]
        : [
            Math.min(box[0], b[0]),
            Math.min(box[1], b[1]),
            Math.max(box[2], b[2]),
            Math.max(box[3], b[3]),
          ];
  }
  return box;
}

/** The picked components after a click on `hit` (additive: Shift or Ctrl). */
export function pickAfterClick(
  picked: readonly number[],
  hit: number,
  additive: boolean,
): number[] {
  if (!additive) {
    return [hit];
  }
  return picked.includes(hit) ? picked.filter((id) => id !== hit) : [...picked, hit];
}

/** A click in the 2D view that missed every vector: picks the component
 * under it, if any. Answers whether one was there. */
export async function pickComponentAt(x: number, y: number, additive: boolean): Promise<boolean> {
  const model = useModelStore.getState();
  const hit = componentUnder(model.components, x, y);
  if (hit === null) {
    if (!additive) {
      clearPickedComponents();
    }
    return false;
  }
  const next = pickAfterClick(pickedComponents(model), hit.id, additive);
  if (!additive && useDocumentStore.getState().selection.ids.length > 0) {
    await useDocumentStore.getState().clearSelection();
  }
  model.setPicked(next);
  model.select(next.includes(hit.id) ? hit.id : (next[next.length - 1] ?? null));
  return true;
}

/** A plain click on a vector: the components are no longer selected. */
export function clearPickedComponents(): void {
  const model = useModelStore.getState();
  if (model.selectedId !== null || model.picked.length > 0) {
    model.setPicked([]);
    model.select(null);
  }
}

setSelectionExtras({
  box: () => {
    const model = useModelStore.getState();
    return boxOf(model.components, pickedComponents(model));
  },
  transform: async (matrix: Matrix, scaleZ?: boolean) => {
    const ids = pickedComponents(useModelStore.getState());
    if (ids.length > 0) {
      await api.model.transformMatrix(ids, matrix, undefined, scaleZ);
      await useModelStore.getState().refresh();
    }
  },
  picked: () => pickedComponents(useModelStore.getState()),
  adopt: async (ids: number[]) => {
    const model = useModelStore.getState();
    await model.refresh();
    if (ids.length > 0) {
      model.setPicked(ids);
      model.select(ids[ids.length - 1] ?? null);
    }
  },
});
