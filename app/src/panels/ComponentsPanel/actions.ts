// Component tree edits: each runs one kernel command (one undo step), then
// brings the document and model mirrors up to date. Answers success; a
// failure shows as the document's last error.

import { api } from '../../bridge/api';
import type { ComponentProps, GroupPatch, TreeTarget } from '../../bridge/componentTree';
import type { PropertiesTarget } from '../ComponentProperties/model';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';

/** Opens the Component Properties form in the left dock on `target`. */
export function openComponentProperties(target: PropertiesTarget): void {
  useModelStore.getState().setPropertiesTarget(target);
  const ui = useUiStore.getState();
  ui.openForm('componentProperties');
}

async function run(action: () => Promise<unknown>): Promise<boolean> {
  let ok = true;
  try {
    await action();
    useDocumentStore.setState({ lastError: null });
  } catch (error) {
    useDocumentStore.setState({
      lastError: error instanceof Error ? error.message : String(error),
    });
    ok = false;
  }
  await useDocumentStore.getState().refresh();
  await useModelStore.getState().refresh();
  return ok;
}

export const treeActions = {
  setProps: (ids: number[], props: ComponentProps) => run(() => api.model.tree.props(ids, props)),
  group: (ids: number[]) =>
    run(async () => {
      await api.model.tree.group(ids);
    }),
  updateGroup: (id: number, patch: GroupPatch) => run(() => api.model.tree.updateGroup(id, patch)),
  ungroup: (id: number) => run(() => api.model.tree.ungroup(id)),
  move: (ids: number[], target: TreeTarget) => run(() => api.model.tree.move(ids, target)),
  setClip: (id: number, vectorIds: number[]) => run(() => api.model.tree.setClip(id, vectorIds)),
  /** Reads the clipping again from `vectorIds`, or the last ones when empty. */
  updateClip: (id: number, vectorIds: number[]) =>
    run(() => api.model.tree.updateClip(id, vectorIds)),
  fromVisible: (ids?: number[]) =>
    run(async () => {
      const reply = await api.model.tree.fromVisible(ids);
      useModelStore.getState().select(reply.id);
    }),
  remove: (ids: number[]) => run(() => api.model.delete(ids)),
  toOtherSide: (ids: number[], copy: boolean) => run(() => api.model.toOtherSide(ids, copy)),
};
