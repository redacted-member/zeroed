// The Model menu items that run at once instead of opening a form: Add Zero
// Plane, Create Component From Visible Model, Create Component from
// Toolpath Preview (Shift keeps only the part over the model, Ctrl only the
// material left over it), Create Component from Bitmap, Export as 3D
// Clipart, Reset Inside or Outside Selected Vectors, Split Selected
// Component, Bake Components (Ctrl keeps the originals) and Selected
// Component Properties. Each kernel change is one undo step; a refusal
// shows as the document's last error.

import { api } from '../../bridge/api';
import type { PreviewComponentMode } from '../../bridge/modelRefine';
import type { SplitMode } from '../../bridge/modelTools';
import { getKernel } from '../../bridge/client';
import { BITMAP_EXTENSIONS } from '../../lib/bitmapCommands';
import { heldModifiers, type HeldModifiers } from '../../lib/heldModifiers';
import { runModelAction } from '../../lib/runModelAction';
import type { AppCommand } from '../../lib/shortcuts';
import { strings } from '../../lib/strings';
import { saveComponentAsClip } from '../../panels/ClipartPanel/saveClip';
import { openComponentProperties, treeActions } from '../../panels/ComponentsPanel/actions';
import { useDocumentStore } from '../../state/documentStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { usePreviewStore } from '../../state/previewStore';

const text = strings.modelShell.menuCommands;

/** Why a reset cannot run with this component and vector selection, or null. */
export function resetBlocker(
  componentId: number | null,
  vectorIds: readonly number[],
): string | null {
  if (componentId === null) {
    return text.needsComponent;
  }
  return vectorIds.length === 0 ? text.needsVectors : null;
}

function fail(message: string): void {
  useDocumentStore.setState({ lastError: message });
}

async function reset(mode: SplitMode): Promise<void> {
  const componentId = useModelStore.getState().selectedId;
  const vectorIds = useDocumentStore.getState().selection.ids;
  const blocker = resetBlocker(componentId, vectorIds);
  if (blocker !== null || componentId === null) {
    fail(blocker ?? text.needsComponent);
    return;
  }
  await runModelAction(() => api.model.split({ componentId, vectorIds: [...vectorIds], mode }));
}

/** What Bake does for the picked components and the keys held: nothing
 * without a component, a baked copy beside the originals with Ctrl, or the
 * originals replaced by one baked component. */
export function bakeMode(
  picked: readonly number[],
  keys: HeldModifiers,
): 'none' | 'copy' | 'replace' {
  if (picked.length === 0) {
    return 'none';
  }
  return keys.ctrl ? 'copy' : 'replace';
}

/** Bakes `ids` into one component, or with Ctrl held adds a baked copy
 * and keeps them. */
export async function bakeComponents(ids: readonly number[]): Promise<void> {
  const mode = bakeMode(ids, heldModifiers());
  if (mode === 'none') {
    fail(text.needsComponent);
    return;
  }
  if (mode === 'copy') {
    await treeActions.fromVisible([...ids]);
    return;
  }
  const reply = await runModelAction(() => api.model.bake([...ids]));
  if (reply !== null) {
    useModelStore.getState().select(reply.id);
  }
}

function openProperties(): void {
  const ids = pickedComponents(useModelStore.getState());
  if (ids.length === 0) {
    fail(text.needsComponent);
    return;
  }
  openComponentProperties({ kind: 'components', ids });
}

async function addZeroPlane(): Promise<void> {
  const reply = await runModelAction(() => api.model.refine.addZeroPlane());
  if (reply !== null) {
    useModelStore.getState().select(reply.id);
  }
}

/** Create Vector Boundary From Selected Components: the silhouette of the
 * picked components as closed vectors, grouped when there are several. */
export async function vectorBoundary(): Promise<void> {
  const ids = pickedComponents(useModelStore.getState());
  if (ids.length === 0) {
    fail(text.needsComponent);
    return;
  }
  const reply = await runModelAction(() => api.model.boundary({ ids, offset: 0, group: true }));
  if (reply !== null) {
    await useDocumentStore.getState().select(reply.ids);
  }
}

/** What part of the toolpath preview becomes the component for the keys
 * held: Shift the part over the model, Ctrl the material left over it,
 * neither the whole preview. */
export function previewModeFor(keys: HeldModifiers): PreviewComponentMode {
  if (keys.shift) {
    return 'overModel';
  }
  return keys.ctrl ? 'leftOver' : 'whole';
}

async function componentFromPreview(): Promise<void> {
  if (usePreviewStore.getState().heightfield === null) {
    fail(text.needsPreview);
    return;
  }
  const mode = previewModeFor(heldModifiers());
  const reply = await runModelAction(() => api.model.refine.fromPreview(mode));
  if (reply !== null) {
    useModelStore.getState().select(reply.id);
  }
}

/** The bitmap Create Component from Bitmap converts: the one selected
 * bitmap, null for none selected (pick a file), or 'refuse' when the
 * selection holds anything else. */
export function bitmapSource(
  selected: ReadonlyArray<{ id: number; kind: string }>,
): number | null | 'refuse' {
  if (selected.length === 0) {
    return null;
  }
  const [only] = selected;
  return selected.length === 1 && only !== undefined && only.kind === 'bitmap' ? only.id : 'refuse';
}

async function componentFromBitmap(): Promise<void> {
  const doc = useDocumentStore.getState();
  const chosen = new Set(doc.selection.ids);
  const source = bitmapSource(doc.entities.filter((e) => chosen.has(e.id)));
  if (source === 'refuse') {
    fail(text.oneBitmap);
    return;
  }
  let path: string | null = null;
  if (source === null) {
    path = await getKernel().chooseOpenPath(text.imageTitle, [
      { name: text.imageFiles, extensions: BITMAP_EXTENSIONS },
    ]);
    if (path === null) {
      return;
    }
  }
  const params = { height: 5, baseHeight: 0, smoothing: 1, invert: false, combine: 'add' as const };
  const reply = await runModelAction(() =>
    api.model.fromBitmap(
      source === null ? { ...params, path: path ?? '' } : { ...params, id: source },
    ),
  );
  if (reply !== null) {
    useModelStore.getState().select(reply.id);
  }
}

async function exportClipart(): Promise<void> {
  const model = useModelStore.getState();
  const component = model.components.find((c) => c.id === model.selectedId) ?? null;
  if (component === null) {
    fail(text.needsComponent);
    return;
  }
  await saveComponentAsClip(component.id, component.name);
}

/** Runs a Model menu command that has no form. Answers false for others. */
export async function runModelMenuCommand(command: AppCommand): Promise<boolean> {
  switch (command) {
    case 'model.addZeroPlane':
      await addZeroPlane();
      return true;
    case 'model.boundary':
      await vectorBoundary();
      return true;
    case 'model.fromPreview':
      await componentFromPreview();
      return true;
    case 'model.fromBitmap':
      await componentFromBitmap();
      return true;
    case 'model.fromVisible':
      await treeActions.fromVisible();
      return true;
    case 'model.exportClipart':
      await exportClipart();
      return true;
    case 'model.resetInside':
      await reset('clearInside');
      return true;
    case 'model.resetOutside':
      await reset('clearOutside');
      return true;
    case 'model.split':
      await reset('split');
      return true;
    case 'model.bake':
      await bakeComponents(pickedComponents(useModelStore.getState()));
      return true;
    case 'model.exportGrayscale': {
      // Lazy: the export helpers read the form strings, kept out of the start chunk.
      const { exportGrayscale } = await import('../../panels/ComponentsPanel/exportStl');
      await exportGrayscale();
      return true;
    }
    case 'model.properties':
      openProperties();
      return true;
    default:
      return false;
  }
}
