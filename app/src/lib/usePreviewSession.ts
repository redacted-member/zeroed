// The React side of PreviewSession: previews refresh the document and model
// mirrors, failures show as the document's last error, and a form that
// closes with a preview up keeps it or takes it back.

import { useEffect, useState } from 'react';

import { api } from '../bridge/api';
import { useDocumentStore } from '../state/documentStore';
import { useModelStore } from '../state/modelStore';
import { PreviewSession, type PreviewCall } from './previewSession';

async function refreshMirrors(): Promise<void> {
  await useDocumentStore.getState().refresh();
  await useModelStore.getState().refresh();
}

function showError(error: unknown): void {
  useDocumentStore.setState({
    lastError: error instanceof Error ? error.message : String(error),
  });
}

export interface PreviewHandle {
  /** Previews `call`, replacing the previous preview. */
  preview: (call: PreviewCall) => Promise<void>;
  /** Keeps what is previewed as its own undo step. */
  keep: () => Promise<void>;
  /** Takes the preview back. */
  discard: () => Promise<void>;
  /** Whether a preview is up and not yet kept. */
  previewing: () => boolean;
}

/** A preview session for one open form. `onClose` says what happens to a
 * preview still up when the form goes away. */
export function usePreviewSession(onClose: 'keep' | 'discard'): PreviewHandle {
  const [current] = useState(
    () =>
      new PreviewSession(async (revision) => {
        await api.model.refine.revertPreview(revision);
        await refreshMirrors();
      }, showError),
  );

  useEffect(
    () => () => {
      if (onClose === 'discard') {
        void current.discard();
      } else {
        void current.keep();
      }
    },
    [current, onClose],
  );

  return {
    preview: (call) =>
      current.run(async (replace) => {
        try {
          const reply = await call(replace);
          useDocumentStore.setState({ lastError: null });
          return reply;
        } finally {
          await refreshMirrors();
        }
      }),
    keep: () => current.keep(),
    discard: () => current.discard(),
    previewing: () => current.previewing,
  };
}
