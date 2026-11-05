// File > Import Bitmap and the bitmap fade: kernel calls plus the store
// refresh, with refusals shown like any other document error.

import { api } from '../bridge/api';
import { getKernel } from '../bridge/client';
import { useDocumentStore } from '../state/documentStore';
import { strings } from './strings';

/** Image files the kernel places and traces. */
export const BITMAP_EXTENSIONS = ['png', 'jpg', 'jpeg', 'bmp', 'webp', 'gif', 'tif', 'tiff'];

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Places an image (asking for one unless given) on the job, selected. Answers the new entity id. */
export async function importBitmap(chosen?: string): Promise<number | null> {
  const path =
    chosen ??
    (await getKernel().chooseOpenPath(strings.bitmaps.importTitle, [
      { name: strings.bitmaps.imageFiles, extensions: BITMAP_EXTENSIONS },
    ]));
  if (path === null) {
    return null;
  }
  const doc = useDocumentStore.getState();
  try {
    const reply = await api.bitmap.import(path);
    useDocumentStore.setState({ lastError: null });
    await doc.refresh();
    return reply.id;
  } catch (error) {
    useDocumentStore.setState({ lastError: message(error) });
    await doc.refresh();
    return null;
  }
}

/** Sets the opacity of bitmaps, 0 to 1, as one undo step. */
export async function fadeBitmaps(ids: number[], opacity: number): Promise<void> {
  try {
    await api.bitmap.setOpacity(ids, opacity);
    useDocumentStore.setState({ lastError: null });
  } catch (error) {
    useDocumentStore.setState({ lastError: message(error) });
  }
  await useDocumentStore.getState().refresh();
}
