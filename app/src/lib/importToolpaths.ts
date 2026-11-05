// File > Import > Import Toolpaths: a G-code file read back as toolpaths,
// one per tool, which then sit in the toolpath list like calculated ones.

import { api } from '../bridge/api';
import { getKernel } from '../bridge/client';
import { useDocumentStore } from '../state/documentStore';
import { useToolpathStore } from '../state/toolpathStore';
import { useUiStore } from '../state/uiStore';
import { strings } from './strings';

/** Files Import Toolpaths offers. */
export const TOOLPATH_FILE_EXTENSIONS: readonly string[] = [
  'nc',
  'tap',
  'gcode',
  'ngc',
  'cnc',
  'txt',
];

/** What the status bar says after an import. */
export function importMessage(count: number, warnings: readonly string[]): string {
  const text = strings.toolpathShell.imported;
  const done = text.imported(count);
  return warnings.length === 0 ? done : `${done} ${text.skipped(warnings.join(', '))}`;
}

/** Asks for a G-code file and imports its toolpaths. */
export async function importToolpaths(): Promise<void> {
  const text = strings.toolpathShell.imported;
  const path = await getKernel().chooseOpenPath(text.importTitle, [
    { name: text.files, extensions: [...TOOLPATH_FILE_EXTENSIONS] },
  ]);
  if (path === null) {
    return;
  }
  try {
    const reply = await api.cam.importToolpaths(path);
    useDocumentStore.setState({ lastError: null });
    await useDocumentStore.getState().refresh();
    await useToolpathStore.getState().refresh();
    const ui = useUiStore.getState();
    ui.showToolpaths();
    const { warningTexts } = await import('./kernelWarnings');
    ui.setStatusHint(importMessage(reply.ids.length, warningTexts(reply.warnings)));
  } catch (error) {
    useDocumentStore.setState({
      lastError: error instanceof Error ? error.message : String(error),
    });
  }
}
