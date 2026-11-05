// Zoom to All and F9: the box holding the material and every object on
// visible layers, and the move that centres the selection on the material.

import type { Bbox } from '../bridge/document';
import { useDocumentStore } from '../state/documentStore';
import { jobBounds, type Bounds2 } from '../views/View2D/camera';

/** The material box grown to take in each object box on a visible layer. */
export function drawingBounds(
  material: Bounds2,
  objects: ReadonlyArray<{ bbox: Bbox | null | undefined; layer: number }>,
  visibleLayers: ReadonlySet<number>,
): Bounds2 {
  const out = { ...material };
  for (const object of objects) {
    const box = object.bbox;
    if (box === null || box === undefined || !visibleLayers.has(object.layer)) {
      continue;
    }
    out.minX = Math.min(out.minX, box[0]);
    out.minY = Math.min(out.minY, box[1]);
    out.maxX = Math.max(out.maxX, box[2]);
    out.maxY = Math.max(out.maxY, box[3]);
  }
  return out;
}

/** The move putting the centre of `selection` on the centre of `material`. */
export function centreOnMaterial(selection: Bbox, material: Bounds2): { dx: number; dy: number } {
  return {
    dx: (material.minX + material.maxX) / 2 - (selection[0] + selection[2]) / 2,
    dy: (material.minY + material.maxY) / 2 - (selection[1] + selection[3]) / 2,
  };
}

/** Zoom to All for the document now: the material and every object on a visible layer. */
export function zoomAllBounds(): Bounds2 {
  const doc = useDocumentStore.getState();
  const visible = new Set(doc.layers.filter((l) => l.visible).map((l) => l.id));
  return drawingBounds(jobBounds(doc.job), doc.entities, visible);
}
