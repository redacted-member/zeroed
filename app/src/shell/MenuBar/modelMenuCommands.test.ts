import { describe, expect, it } from 'vitest';

import { strings } from '../../lib/strings';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import { usePreviewStore } from '../../state/previewStore';
import { useUiStore } from '../../state/uiStore';
import {
  bakeMode,
  bitmapSource,
  previewModeFor,
  resetBlocker,
  runModelMenuCommand,
} from './modelMenuCommands';

describe('model menu commands', () => {
  it('needs a component and vectors to reset', () => {
    const text = strings.modelShell.menuCommands;
    expect(resetBlocker(null, [1])).toBe(text.needsComponent);
    expect(resetBlocker(4, [])).toBe(text.needsVectors);
    expect(resetBlocker(4, [1, 2])).toBeNull();
  });

  it('leaves commands it does not own to the others', async () => {
    expect(await runModelMenuCommand('model.slice')).toBe(false);
  });

  it('bakes the picked components, or with Ctrl keeps them and adds a copy', async () => {
    expect(bakeMode([], { shift: false, ctrl: false })).toBe('none');
    expect(bakeMode([4], { shift: false, ctrl: false })).toBe('replace');
    expect(bakeMode([4, 5], { shift: false, ctrl: true })).toBe('copy');
    useModelStore.setState({ selectedId: null, picked: [] });
    expect(await runModelMenuCommand('model.bake')).toBe(true);
    expect(useDocumentStore.getState().lastError).toBe(
      strings.modelShell.menuCommands.needsComponent,
    );
  });

  it('opens Component Properties on the picked components from the menu', async () => {
    useModelStore.setState({ selectedId: 4, picked: [4, 6], propertiesTarget: null });
    expect(await runModelMenuCommand('model.properties')).toBe(true);
    expect(useModelStore.getState().propertiesTarget).toEqual({ kind: 'components', ids: [4, 6] });
    expect(useUiStore.getState().activeForm).toBe('componentProperties');
  });

  it('splits at once, asking for a component and a vector first', async () => {
    useModelStore.setState({ selectedId: null, picked: [] });
    expect(await runModelMenuCommand('model.split')).toBe(true);
    expect(useDocumentStore.getState().lastError).toBe(
      strings.modelShell.menuCommands.needsComponent,
    );
  });

  it('asks for a component before drawing a vector boundary', async () => {
    useModelStore.setState({ selectedId: null, picked: [] });
    expect(await runModelMenuCommand('model.boundary')).toBe(true);
    expect(useDocumentStore.getState().lastError).toBe(
      strings.modelShell.menuCommands.needsComponent,
    );
  });
  it('picks the preview part from the keys held', () => {
    expect(previewModeFor({ shift: false, ctrl: false })).toBe('whole');
    expect(previewModeFor({ shift: true, ctrl: false })).toBe('overModel');
    expect(previewModeFor({ shift: false, ctrl: true })).toBe('leftOver');
  });

  it('needs a toolpath preview before making a component of it', async () => {
    usePreviewStore.setState({ heightfield: null });
    expect(await runModelMenuCommand('model.fromPreview')).toBe(true);
    expect(useDocumentStore.getState().lastError).toBe(
      strings.modelShell.menuCommands.needsPreview,
    );
  });

  it('converts one selected bitmap, or asks for a file with nothing selected', async () => {
    expect(bitmapSource([])).toBeNull();
    expect(bitmapSource([{ id: 7, kind: 'bitmap' }])).toBe(7);
    expect(bitmapSource([{ id: 7, kind: 'path' }])).toBe('refuse');
    expect(
      bitmapSource([
        { id: 7, kind: 'bitmap' },
        { id: 8, kind: 'bitmap' },
      ]),
    ).toBe('refuse');
    useDocumentStore.setState({
      selection: { ...useDocumentStore.getState().selection, ids: [3] },
      entities: [{ id: 3, kind: 'path', layer: 1, bbox: [0, 0, 1, 1], closed: true, revision: 1 }],
    });
    expect(await runModelMenuCommand('model.fromBitmap')).toBe(true);
    expect(useDocumentStore.getState().lastError).toBe(strings.modelShell.menuCommands.oneBitmap);
  });
});
