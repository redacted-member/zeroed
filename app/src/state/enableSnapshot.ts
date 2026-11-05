// The live input of lib/enableState: read from the stores, once outside
// React or as a hook that follows them.

import { useMemo } from 'react';

import { isTauriHost } from '../bridge';
import type { EnableState } from '../lib/enableState';
import { isRotary } from '../lib/rotary';
import { isDoubleSided } from '../lib/sides';
import { useClipboardStore } from './clipboardStore';
import { useDocumentStore } from './documentStore';
import { pickedComponents, useModelStore } from './modelStore';
import { usePrefsStore } from './prefsStore';
import { useToolpathStore } from './toolpathStore';

/** The enable state of the app now. */
export function currentEnableState(): EnableState {
  const doc = useDocumentStore.getState();
  return {
    vectors: doc.selection.ids.length,
    components: pickedComponents(useModelStore.getState()).length,
    canUndo: doc.undo.canUndo,
    canRedo: doc.undo.canRedo,
    clipboard: useClipboardStore.getState().count,
    toolpathSelected: useToolpathStore.getState().selectedId !== null,
    draw2dPreviews: usePrefsStore.getState().prefs.toolpaths.draw2dPreviews,
    doubleSided: isDoubleSided(doc.job),
    rotary: isRotary(doc.job),
    native: isTauriHost(),
  };
}

/** The enable state, kept current by the stores. */
export function useEnableState(): EnableState {
  const vectors = useDocumentStore((s) => s.selection.ids.length);
  const components = useModelStore((s) => pickedComponents(s).length);
  const canUndo = useDocumentStore((s) => s.undo.canUndo);
  const canRedo = useDocumentStore((s) => s.undo.canRedo);
  const clipboard = useClipboardStore((s) => s.count);
  const toolpathSelected = useToolpathStore((s) => s.selectedId !== null);
  const draw2dPreviews = usePrefsStore((s) => s.prefs.toolpaths.draw2dPreviews);
  const doubleSided = useDocumentStore((s) => isDoubleSided(s.job));
  const rotary = useDocumentStore((s) => isRotary(s.job));
  return useMemo(
    () => ({
      vectors,
      components,
      canUndo,
      canRedo,
      clipboard,
      toolpathSelected,
      draw2dPreviews,
      doubleSided,
      rotary,
      native: isTauriHost(),
    }),
    [
      vectors,
      components,
      canUndo,
      canRedo,
      clipboard,
      toolpathSelected,
      draw2dPreviews,
      doubleSided,
      rotary,
    ],
  );
}
