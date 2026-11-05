// Toolpath list actions shared by the list and the Toolpaths panel's
// management icons (Edit, Duplicate, Delete Toolpath).

import type { ToolpathSummary } from '../../bridge/cam';
import { strings } from '../../lib/strings';
import { useDocumentStore } from '../../state/documentStore';
import { useGadgetStore } from '../../state/gadgetStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { formForToolpath } from './editForm';
import { sheetForEdit } from './tree';

/** Opens `toolpath` in its form, first activating its sheet when the preference asks. */
export function openToolpathEditor(toolpath: ToolpathSummary): void {
  const doc = useDocumentStore.getState();
  const activeSheet = doc.sheets.find((s) => s.active)?.id ?? 0;
  const sheet = sheetForEdit(
    toolpath,
    activeSheet,
    usePrefsStore.getState().prefs.toolpaths.selectSheetOnEdit,
  );
  if (sheet !== null) {
    void doc.setActiveSheet(sheet);
  }
  if (typeof toolpath.gadget === 'string') {
    useGadgetStore.getState().choose(toolpath.gadget);
  }
  const form = formForToolpath(toolpath);
  if (form !== null) {
    useUiStore.getState().editToolpath(form, toolpath.id);
  }
}

/** The toolpath selected in the list, if any. */
export function selectedToolpath(): ToolpathSummary | null {
  const store = useToolpathStore.getState();
  return store.toolpaths.find((t) => t.id === store.selectedId) ?? null;
}

/** Duplicates `toolpath` as "<name> copy". */
export async function duplicateToolpath(toolpath: ToolpathSummary): Promise<void> {
  await useToolpathStore
    .getState()
    .duplicate(toolpath.id, `${toolpath.name} ${strings.panelMenus.copySuffix}`);
}
