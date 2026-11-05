// The component preview of the 2D view: the composite model drawn as a
// shaded grey picture under the bitmaps and vectors, so the relief can be
// drawn over in 2D. The model grid's row 0 is its lowest Y, so the picture
// is built top row first and placed with Y flipped.

import type { Composite } from '../../bridge/model';
import { shadeComposite } from './componentShade';
import { shadeInWorker } from './componentShadeWorker';

export { shadeComposite };

/** The job space affine [a, b, c, d, e, f] that places the picture's pixels on the grid. */
export function previewAffine(
  composite: Pick<Composite, 'height' | 'cellSize' | 'originX' | 'originY'>,
): [number, number, number, number, number, number] {
  const { height, cellSize, originX, originY } = composite;
  return [cellSize, 0, 0, -cellSize, originX - cellSize / 2, originY + (height - 0.5) * cellSize];
}

/** Models of more cells than this are shaded in the worker; smaller ones
 * take a few milliseconds in place. */
const WORKER_CELLS = 250_000;

interface Picture {
  image: CanvasImageSource;
  affine: [number, number, number, number, number, number];
}

/** The picture of `composite` shaded in place, or null without a canvas. */
function shadeHere(composite: Composite): Picture | null {
  const canvas = document.createElement('canvas');
  canvas.width = composite.width;
  canvas.height = composite.height;
  const context = canvas.getContext('2d');
  if (context === null) {
    return null;
  }
  const image = context.createImageData(composite.width, composite.height);
  shadeComposite(composite, image.data);
  context.putImageData(image, 0, 0);
  return { image: canvas, affine: previewAffine(composite) };
}

/** Holds the shaded picture of the latest composite and draws it. A large
 * model is shaded in a worker: the last picture stays until the new one
 * is ready, then `onReady` asks for a redraw. */
export class ComponentPreviewLayer {
  private picture: Picture | null = null;
  /** Counts the pictures asked for, so only the latest one is kept. */
  private asked = 0;

  constructor(private readonly onReady: () => void = () => undefined) {}

  /** Replaces the picture; null clears it. */
  set(composite: Composite | null): void {
    this.asked += 1;
    const ask = this.asked;
    if (composite === null || composite.width === 0 || composite.height === 0) {
      this.drop();
      return;
    }
    if (composite.width * composite.height <= WORKER_CELLS) {
      this.keep(shadeHere(composite));
      return;
    }
    const affine = previewAffine(composite);
    void shadeInWorker(composite).then((bitmap) => {
      if (ask !== this.asked) {
        bitmap?.close();
        return;
      }
      this.keep(bitmap === null ? shadeHere(composite) : { image: bitmap, affine });
      this.onReady();
    });
  }

  private keep(picture: Picture | null): void {
    this.drop();
    this.picture = picture;
  }

  private drop(): void {
    const image = this.picture?.image;
    if (typeof ImageBitmap !== 'undefined' && image instanceof ImageBitmap) {
      image.close();
    }
    this.picture = null;
  }

  /** Draws the picture. The context must be in job space. */
  draw(context: CanvasRenderingContext2D): void {
    const picture = this.picture;
    if (picture === null) {
      return;
    }
    context.save();
    context.transform(...picture.affine);
    context.imageSmoothingEnabled = true;
    context.drawImage(picture.image, 0, 0);
    context.restore();
  }
}
