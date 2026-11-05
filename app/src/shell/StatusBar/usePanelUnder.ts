// The furniture panel under the 2D pointer for the status line ("Door 1,
// front face"): asked of the kernel one question at a time, the
// newest point waiting. A kernel without furniture.panelOf is not asked
// again.

import { useEffect } from 'react';

import { api } from '../../bridge/api';
import type { PanelUnder } from '../../bridge/furniture';
import { KernelError } from '../../bridge/types';
import { strings } from '../../lib/strings';
import { useDocumentStore } from '../../state/documentStore';
import { useFaceEditStore } from '../../state/faceEditStore';
import { LatestQueue } from '../../state/latestQueue';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';

let answered = true;

/** The status line's words for the panel under the pointer: its name and the face seen from above. */
export function panelLabel(under: PanelUnder): string | null {
  if (under.name === null || under.top === null) {
    return null;
  }
  return strings.statusBar.panelFace(under.name, under.top);
}

const queue = new LatestQueue<{ sheet: number; point: [number, number] }, string | null>(
  async ({ sheet, point }) => panelLabel(await api.furniture.panelOf(sheet, point)),
  (label) => useFaceEditStore.getState().setPointerPanel(label),
  (error) => {
    if (error instanceof KernelError && error.code === 'unknown_command') {
      answered = false;
    }
    useFaceEditStore.getState().setPointerPanel(null);
  },
);

/** Keeps the status line's panel name in step with the 2D pointer. */
export function usePanelUnder(): void {
  const cursor = useViewStore((state) => state.cursor);
  const sheet = useDocumentStore((state) => state.sheets.find((s) => s.active)?.id ?? null);
  const twoD = useUiStore((state) => state.viewMode !== '3d');
  useEffect(() => {
    if (!answered || !twoD) {
      return;
    }
    if (cursor === null || sheet === null) {
      queue.cancel();
      useFaceEditStore.getState().setPointerPanel(null);
      return;
    }
    queue.push({ sheet, point: [cursor.x, cursor.y] });
  }, [cursor, sheet, twoD]);
}
