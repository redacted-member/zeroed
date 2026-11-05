// The selected components a move, scale or rotate drag of the selection
// carries: their boxes, and a picture of them alone that the drag preview
// draws moving with the vectors.

import { api } from '../../../bridge/api';
import type { Bbox } from '../../../bridge/document';
import type { ComponentSummary } from '../../../bridge/model';
import { pickedComponents, useModelStore } from '../../../state/modelStore';
import type { ToolHost } from './types';

/** Boxes of the visible components among `ids`, in list order. */
export function carriedBoxes(
  components: readonly ComponentSummary[],
  ids: readonly number[],
): Bbox[] {
  const boxes: Bbox[] = [];
  for (const c of components) {
    if (ids.includes(c.id) && c.visible && c.bbox !== undefined) {
      boxes.push(c.bbox);
    }
  }
  return boxes;
}

/** Key naming the picture of `ids` at one state of the model. */
export function carriedKey(ids: readonly number[], stamp: number | null): string {
  return `${ids.join(',')}@${stamp ?? ''}`;
}

/**
 * The boxes of the selected components a drag starting now carries, empty
 * for none. Fetches their picture for the preview when the selection or the
 * model changed since the last one; it shows once it arrives.
 */
export function carriedComponents(host: ToolHost): Bbox[] {
  const model = useModelStore.getState();
  const ids = pickedComponents(model);
  const boxes = carriedBoxes(model.components, ids);
  if (boxes.length === 0) {
    return boxes;
  }
  const key = carriedKey(ids, model.stamp);
  if (host.renderer.dragComponentsKey !== key) {
    host.renderer.setDragComposite(null, key);
    void api.model
      .composite(ids)
      .then((composite) => {
        if (host.renderer.dragComponentsKey === key) {
          host.renderer.setDragComposite(composite, key);
          host.requestDraw();
        }
      })
      .catch(() => undefined);
  }
  return boxes;
}
