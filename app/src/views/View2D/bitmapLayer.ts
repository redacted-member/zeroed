// Draws placed bitmaps under the vectors of the 2D view. Each image is
// fetched once from the kernel (bitmaps.pixels), decoded by the browser and
// kept by image id; the entity's corner quad, taken from its display run,
// places it with one affine transform, so moved, scaled and rotated bitmaps
// draw without refetching.

import { api } from '../../bridge/api';
import { useTraceViewStore } from '../../state/traceViewStore';
import type { DisplayEntry } from './displayCache';

/** Canvas `transform` arguments. */
export type Affine = [number, number, number, number, number, number];

/**
 * The transform from image pixels (origin at the top left, Y down) to job
 * millimetres for a quad run of x, y pairs in the order lower left, lower
 * right, upper right, upper left. Null for a run with fewer than four
 * corners or an empty image.
 */
export function imageAffine(run: Float32Array, width: number, height: number): Affine | null {
  if (run.length < 8 || width <= 0 || height <= 0) {
    return null;
  }
  const at = (i: number) => run[i] ?? 0;
  const [x0, y0, x1, y1, x3, y3] = [at(0), at(1), at(2), at(3), at(6), at(7)];
  return [(x1 - x0) / width, (y1 - y0) / width, (x0 - x3) / height, (y0 - y3) / height, x3, y3];
}

type Slot = { state: 'loading' } | { state: 'failed' } | { state: 'ready'; image: ImageBitmap };

export class BitmapLayer {
  private readonly images = new Map<number, Slot>();

  /** `onLoad` is called when an image arrives, to draw it. */
  constructor(private readonly onLoad: () => void) {}

  /** Draws the bitmaps among `entries`. The context must be in job space. */
  draw(context: CanvasRenderingContext2D, entries: readonly DisplayEntry[]): void {
    const override = useTraceViewStore.getState().override;
    for (const entry of entries) {
      if (entry.kind !== 'bitmap' || entry.imageId === undefined) {
        continue;
      }
      const slot = this.images.get(entry.imageId);
      if (slot === undefined) {
        this.load(entry.imageId);
        continue;
      }
      const run = entry.runs[0];
      if (slot.state !== 'ready' || run === undefined) {
        continue;
      }
      const image = override?.imageId === entry.imageId ? override.image : slot.image;
      const affine = imageAffine(run, image.width, image.height);
      if (affine === null) {
        continue;
      }
      context.save();
      context.globalAlpha = Math.min(1, Math.max(0, entry.opacity ?? 1));
      context.transform(...affine);
      context.imageSmoothingEnabled = true;
      context.drawImage(image, 0, 0);
      context.restore();
    }
  }

  private load(imageId: number): void {
    this.images.set(imageId, { state: 'loading' });
    api.bitmap
      .pixels(imageId)
      .then((blob) => createImageBitmap(blob))
      .then((image) => {
        this.images.set(imageId, { state: 'ready', image });
        this.onLoad();
      })
      .catch(() => {
        this.images.set(imageId, { state: 'failed' });
      });
  }
}
