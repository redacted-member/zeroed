// View > Save Shaded Image: a picture of the 3D view as it is drawn. With a
// shaded background the picture carries it; without one the background is
// left clear (transparent in a PNG, white in a JPEG or BMP). Written by the
// kernel like Save 2D View as Image.

import { exportsApi } from '../bridge/exports';
import { getKernel } from '../bridge/client';
import type { View3dPrefs } from '../bridge/prefs';
import { useDocumentStore } from '../state/documentStore';
import { usePrefsStore } from '../state/prefsStore';
import { fileName } from './fileCommands';
import { strings } from './strings';
import { VIEW_IMAGE_FILTERS, viewImagePath } from './viewImage';

const text = strings.shadedImage;

/** Renders the 3D view now and answers its canvas, or null when it is not showing. */
export type ShadedSnapshot = () => HTMLCanvasElement | null;

let snapshot: ShadedSnapshot | null = null;

/** The 3D view registers how to capture itself while it is mounted. */
export function registerShadedSnapshot(capture: ShadedSnapshot | null): void {
  snapshot = capture;
}

/** Default file name: the job's name without its extension, then "3D View". */
export function shadedImageName(jobPath: string | null): string {
  const stem = fileName(jobPath).replace(/\.[^.]*$/, '');
  return `${stem} ${text.suffix}.png`;
}

/** The top and bottom colours of the background the picture carries, or null for none. */
export function backgroundOf(view3d: View3dPrefs): [string, string] | null {
  if (view3d.background === 'theme') {
    return null;
  }
  const bottom = view3d.background === 'solid' ? view3d.topColour : view3d.bottomColour;
  return [view3d.topColour, bottom];
}

function compose(view: HTMLCanvasElement, background: [string, string] | null): string {
  if (background === null) {
    return view.toDataURL('image/png');
  }
  const out = document.createElement('canvas');
  out.width = view.width;
  out.height = view.height;
  const context = out.getContext('2d');
  if (context === null) {
    return view.toDataURL('image/png');
  }
  const gradient = context.createLinearGradient(0, 0, 0, out.height);
  gradient.addColorStop(0, background[0]);
  gradient.addColorStop(1, background[1]);
  context.fillStyle = gradient;
  context.fillRect(0, 0, out.width, out.height);
  context.drawImage(view, 0, 0);
  return out.toDataURL('image/png');
}

/** Captures the 3D view and saves it where the user chooses. */
export async function saveShadedImage(): Promise<void> {
  const view = snapshot?.() ?? null;
  if (view === null) {
    useDocumentStore.setState({ lastError: text.noView });
    return;
  }
  const data = compose(view, backgroundOf(usePrefsStore.getState().prefs.view3d));
  const name = shadedImageName(useDocumentStore.getState().path);
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
