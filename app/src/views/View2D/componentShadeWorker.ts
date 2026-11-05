// The main thread side of the shading worker: one worker, started on the
// first large model, answering each request with an ImageBitmap (or null
// when it cannot, and the caller shades in place instead).

import type { Composite } from '../../bridge/model';

export type ShadeInput = Pick<Composite, 'width' | 'height' | 'cellSize' | 'z'>;

export interface ShadeRequest {
  id: number;
  composite: ShadeInput;
}

export interface ShadeReply {
  id: number;
  bitmap: ImageBitmap | null;
}

let worker: Worker | null | undefined;
let nextId = 1;
const waiting = new Map<number, (bitmap: ImageBitmap | null) => void>();

function shader(): Worker | null {
  if (worker !== undefined) {
    return worker;
  }
  if (typeof Worker === 'undefined' || typeof createImageBitmap === 'undefined') {
    worker = null;
    return worker;
  }
  try {
    const started = new Worker(new URL('./componentShade.worker.ts', import.meta.url), {
      type: 'module',
    });
    started.onmessage = (event: MessageEvent<ShadeReply>) => {
      const done = waiting.get(event.data.id);
      waiting.delete(event.data.id);
      done?.(event.data.bitmap);
    };
    started.onerror = () => {
      // A worker that fails stays off; every caller shades in place.
      worker = null;
      for (const done of waiting.values()) {
        done(null);
      }
      waiting.clear();
    };
    worker = started;
  } catch {
    worker = null;
  }
  return worker;
}

/** The shaded picture of `composite` made in the worker, or null when
 * there is no worker to make it. */
export function shadeInWorker(composite: ShadeInput): Promise<ImageBitmap | null> {
  const target = shader();
  if (target === null) {
    return Promise.resolve(null);
  }
  const id = nextId;
  nextId += 1;
  const { width, height, cellSize, z } = composite;
  return new Promise((resolve) => {
    waiting.set(id, resolve);
    const request: ShadeRequest = { id, composite: { width, height, cellSize, z } };
    target.postMessage(request);
  });
}
