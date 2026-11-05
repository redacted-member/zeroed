// Runs a model command from a form the way the model store's own actions
// do: the error (or none) goes to the document store, then both mirrors
// refresh so the lists, the 2D view and the 3D view catch up.

import { useDocumentStore } from '../state/documentStore';
import { useModelStore } from '../state/modelStore';

/** The action's result, or null when the kernel refused it. */
export async function runModelAction<T>(action: () => Promise<T>): Promise<T | null> {
  let result: T | null = null;
  try {
    result = await action();
    useDocumentStore.setState({ lastError: null });
  } catch (error) {
    useDocumentStore.setState({
      lastError: error instanceof Error ? error.message : String(error),
    });
  }
  await useDocumentStore.getState().refresh();
  await useModelStore.getState().refresh();
  return result;
}
