// Live preview for the text forms: asks the kernel to lay the text out
// without creating anything, a short moment after the last change, and
// publishes the outlines as the dashed preview.

import { useEffect, useState } from 'react';

import { api } from '../bridge/formApi';
import type { Path, TextPreviewArgs } from '../bridge/document';
import { usePreview } from './useDrawController';

/** Delay after the last change before laying the text out again. */
export const PREVIEW_DELAY_MS = 120;

const NONE: Path[] = [];

export function useTextPreview(args: TextPreviewArgs | null): void {
  const key = args === null ? '' : JSON.stringify(args);
  // The last layout and the request it answers. While a new layout is on its
  // way the previous one stays up, so typing does not make the preview blink.
  const [result, setResult] = useState<{ key: string; paths: Path[] }>({ key: '', paths: NONE });

  useEffect(() => {
    if (key === '') {
      return undefined;
    }
    let live = true;
    const timer = window.setTimeout(() => {
      void api.text
        .preview(JSON.parse(key) as TextPreviewArgs)
        .then((reply) => reply.paths)
        .catch(() => NONE)
        .then((paths) => {
          if (live) {
            setResult({ key, paths });
          }
        });
    }, PREVIEW_DELAY_MS);
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [key]);

  usePreview(key === '' || result.key === '' ? NONE : result.paths);
}
