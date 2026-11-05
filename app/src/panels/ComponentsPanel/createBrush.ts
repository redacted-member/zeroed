// Create Sculpting Brush: writes a component into the brushes folder, where
// the Sculpt form's Load Brush looks first.

import { api } from '../../bridge/formApi';
import { strings } from '../../lib/formStrings';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';

export async function createSculptingBrush(id: number): Promise<void> {
  try {
    const reply = await api.model.createBrush(id);
    useDocumentStore.setState({ lastError: null });
    useUiStore.getState().setStatusHint(strings.model.treeMenu.brushSaved(reply.path));
  } catch (error) {
    useDocumentStore.setState({
      lastError: error instanceof Error ? error.message : String(error),
    });
  }
}
