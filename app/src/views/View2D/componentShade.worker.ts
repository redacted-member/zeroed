// Shades a composite for the 2D component preview off the main thread and
// hands back the picture, so a large model does not hold up the view.

import { shadeComposite } from './componentShade';
import type { ShadeReply, ShadeRequest } from './componentShadeWorker';

interface Scope {
  onmessage: ((event: MessageEvent<ShadeRequest>) => void) | null;
  postMessage: (message: ShadeReply, transfer: Transferable[]) => void;
}

const scope = self as unknown as Scope;

scope.onmessage = (event) => {
  const { id, composite } = event.data;
  const image = new ImageData(composite.width, composite.height);
  shadeComposite(composite, image.data);
  createImageBitmap(image).then(
    (bitmap) => scope.postMessage({ id, bitmap }, [bitmap]),
    () => scope.postMessage({ id, bitmap: null }, []),
  );
};
