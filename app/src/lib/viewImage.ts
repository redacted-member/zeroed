// Save 2D View as Image: renders the 2D view's canvas to a PNG. In the
// desktop shell and the dev host the kernel writes it (as PNG, JPEG, BMP or GIF
// by the extension chosen).

import { exportsApi } from '../bridge/exports';
import { getKernel } from '../bridge/client';
import type { FileFilter } from '../bridge/types';
import { useDocumentStore } from '../state/documentStore';
import { fileName, withExtension } from './fileCommands';
import { strings } from './strings';

const text = strings.viewImage;

/** Extensions the kernel writes a view image as. */
export const VIEW_IMAGE_EXTENSIONS: readonly string[] = ['png', 'jpg', 'jpeg', 'bmp', 'gif'];

export const VIEW_IMAGE_FILTERS: FileFilter[] = [
  { name: text.pngFiles, extensions: ['png'] },
  { name: text.jpegFiles, extensions: ['jpg', 'jpeg'] },
  { name: text.bmpFiles, extensions: ['bmp'] },
  { name: text.gifFiles, extensions: ['gif'] },
];

/** Default file name: the job's name without its extension, then "2D View". */
export function viewImageName(jobPath: string | null): string {
  const stem = fileName(jobPath).replace(/\.[^.]*$/, '');
  return `${stem} ${text.suffix}.png`;
}

/** The path with `.png` added unless it already names a format we write. */
export function viewImagePath(path: string): string {
  return withExtension(path, 'png', VIEW_IMAGE_EXTENSIONS);
}

/** The 2D view's canvas, when the 2D view is showing. */
function viewCanvas(): HTMLCanvasElement | null {
  return document.querySelector<HTMLCanvasElement>('canvas.view-2d__canvas');
}

/** Renders the 2D view to a PNG and saves it where the user chooses. */
export async function saveViewImage(): Promise<void> {
  const canvas = viewCanvas();
  if (canvas === null) {
    useDocumentStore.setState({ lastError: text.noView });
    return;
  }
  const data = canvas.toDataURL('image/png');
  const name = viewImageName(useDocumentStore.getState().path);
  const kernel = getKernel();
  const chosen = await kernel.chooseSavePath(text.dialogTitle, VIEW_IMAGE_FILTERS, name);
  if (chosen === null) {
    return;
  }
  try {
    await exportsApi.saveImage(viewImagePath(chosen), data);
    useDocumentStore.setState({ lastError: null });
  } catch (error) {
    useDocumentStore.setState({
      lastError: error instanceof Error ? error.message : String(error),
    });
  }
}
