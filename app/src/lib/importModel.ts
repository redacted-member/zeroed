// File > Import > Import Component / 3D Model and Model > Import Component.
// Loaded with the forms (it uses their strings), never at start.

import { getKernel } from '../bridge/client';
import { usePendingFileStore } from '../state/pendingFileStore';
import { useUiStore } from '../state/uiStore';
import { strings } from './formStrings';

const text = strings.model.importModel;

/** The open panel for a model file; null when none is chosen. */
export function chooseMesh(): Promise<string | null> {
  return getKernel().chooseOpenPath(text.dialogTitle, [
    { name: text.meshFiles, extensions: ['stl', 'obj', '3ds', 'dxf', '3dm'] },
  ]);
}

/**
 * Opens the import form, which asks for the file. With the import already
 * showing, asks for the file here and loads it into a fresh form; a
 * cancelled choice leaves the import as it was.
 */
export async function importModelCommand(): Promise<void> {
  const ui = useUiStore.getState();
  ui.setActiveTab('design');
  if (ui.activeForm !== 'importModel' && ui.activeForm !== 'positionModel') {
    ui.openForm('importModel');
    return;
  }
  const chosen = await chooseMesh();
  if (chosen === null) {
    return;
  }
  usePendingFileStore.getState().offer('mesh', chosen);
  const now = useUiStore.getState();
  now.closeForm();
  now.openForm('importModel');
}
