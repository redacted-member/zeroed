// The window title names the open job
// title bar: "sign.carve - CarveStudio", or "Untitled - CarveStudio".

import { getKernel } from '../bridge/client';
import { useDocumentStore } from '../state/documentStore';
import { fileName } from './fileCommands';
import { strings } from './strings';

/** The window title for the job at `path` (null before its first save). */
export function windowTitle(path: string | null): string {
  return `${fileName(path)} - ${strings.app.name}`;
}

function show(title: string): void {
  const kernel = getKernel();
  if (kernel.setWindowTitle === undefined) {
    document.title = title;
  } else {
    void kernel.setWindowTitle(title);
  }
}

/**
 * Keeps the window title on the open job's file name, and where the host
 * can, marks the window edited while the job has unsaved changes (the dot
 * in the close button) and names its file (Command-click on the title).
 * Returns the unsubscribe.
 */
export function watchWindowTitle(): () => void {
  let shown: string | null = null;
  let state: string | null = null;
  const update = (path: string | null, dirty: boolean) => {
    const title = windowTitle(path);
    if (title !== shown) {
      shown = title;
      show(title);
    }
    const next = `${dirty ? 1 : 0}${path ?? ''}`;
    if (next !== state) {
      state = next;
      void getKernel().setDocumentState?.(dirty, path);
    }
  };
  const { path, dirty } = useDocumentStore.getState();
  update(path, dirty);
  return useDocumentStore.subscribe((doc) => update(doc.path, doc.dirty));
}
