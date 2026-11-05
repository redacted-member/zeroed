// Reimport from the component tree: reopens the import flow with the
// settings a component was imported with. A segmented import goes back to
// 3D Segmenting as it was left; the new components replace the old ones.

import { api } from '../bridge/formApi';
import { useDocumentStore } from '../state/documentStore';
import { useImportStore } from '../state/importStore';
import { useUiStore } from '../state/uiStore';

/** Answers whether the flow opened. */
export async function reimportComponent(id: number): Promise<boolean> {
  try {
    const { source, ids } = await api.model.import.settings(id);
    if (source === null) {
      return false;
    }
    const ui = useUiStore.getState();
    ui.closeForm();
    useImportStore.getState().resume(source, ids);
    ui.openForm(source.plan ? 'segmentModel' : 'importModel');
    return true;
  } catch (error) {
    useDocumentStore.setState({
      lastError: error instanceof Error ? error.message : String(error),
    });
    return false;
  }
}
