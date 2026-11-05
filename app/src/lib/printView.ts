// File > Print, Print Preview and Print Setup: the 2D view is captured as
// an image, which views/PrintView lays out alone on the printed page, and
// the system print dialog (which holds the page setup) takes over.

import { useDialogStore } from '../state/dialogStore';
import { useDocumentStore } from '../state/documentStore';
import { strings } from './strings';

/** The 2D view as a PNG data URL, or null when the 2D view is not showing. */
export function captureView(): string | null {
  const canvas = document.querySelector<HTMLCanvasElement>('canvas.view-2d__canvas');
  return canvas === null ? null : canvas.toDataURL('image/png');
}

function capture(): boolean {
  const image = captureView();
  if (image === null) {
    useDocumentStore.setState({ lastError: strings.printing.noView });
    return false;
  }
  useDialogStore.getState().setPrintImage(image);
  return true;
}

/** Prints the 2D view through the system print dialog. */
export function printView(): void {
  if (!capture()) {
    return;
  }
  // Let the print layout render the new image first.
  requestAnimationFrame(() => requestAnimationFrame(() => window.print()));
}

/** Shows the page as it will print, with a Print button. */
export function printPreview(): void {
  if (capture()) {
    useDialogStore.getState().show('printPreview');
  }
}
