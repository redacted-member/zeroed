import { beforeAll, describe, expect, it } from 'vitest';
import { loadExtraIcons, toolIcon } from '../../shell/ToolIcons/ToolIcons';

import type { Layer } from '../../bridge/document';
import { entriesWithoutIcon, menuItems } from '../../views/PopupMenu/model';
import { buildLayerMenu, reorderedIds, shownLayers } from './menu';

const layer: Layer = {
  id: 1,
  name: 'Layer 1',
  colour: '#000000',
  visible: true,
  locked: false,
  active: true,
  count: 3,
};

// The menu and form icons load lazily in the app.
beforeAll(loadExtraIcons);

describe('layer menu', () => {
  it('offers every layer command and follows the layer state', () => {
    const items = menuItems(buildLayerMenu({ layer, layerCount: 2, hasSelection: true }));
    expect(items.map((i) => i.action)).toEqual([
      'rename',
      'toggleVisible',
      'toggleLocked',
      'toggleKeepOut',
      'makeActive',
      'selectAll',
      'moveSelection',
      'duplicate',
      'mergeIntoActive',
      'delete',
      'newLayer',
      'insertLayer',
      'toggleHideEmpty',
    ]);
    expect(items.find((i) => i.id === 'merge')?.disabled).toBe(true);
    const other = menuItems(
      buildLayerMenu({ layer: { ...layer, active: false }, layerCount: 2, hasSelection: false }),
    );
    expect(other.find((i) => i.id === 'merge')?.disabled).toBe(false);
    expect(items.find((i) => i.id === 'visible')?.label).toBe('Hide');
    expect(items.find((i) => i.id === 'active')?.disabled).toBe(true);
    expect(items.find((i) => i.id === 'selectAll')?.disabled).toBe(false);
  });

  it('marks and clears a keep-out layer', () => {
    const label = (keepOut: boolean) =>
      menuItems(
        buildLayerMenu({ layer: { ...layer, keepOut }, layerCount: 2, hasSelection: false }),
      ).find((i) => i.id === 'keepOut')?.label;
    expect(label(false)).toBe('Keep-out Zone');
    expect(label(true)).toBe('Not a Keep-out Zone');
  });

  it('disables what cannot apply', () => {
    const items = menuItems(
      buildLayerMenu({
        layer: { ...layer, locked: true, count: 0 },
        layerCount: 1,
        hasSelection: false,
      }),
    );
    const disabled = (id: string) => items.find((i) => i.id === id)?.disabled;
    expect(disabled('selectAll')).toBe(true);
    expect(disabled('moveSelection')).toBe(true);
    expect(disabled('delete')).toBe(true);
    expect(items.find((i) => i.id === 'locked')?.label).toBe('Unlock');
  });
});

describe('layer menu icons', () => {
  it('draws an icon on every item, Hide empty layers beside its check mark', () => {
    const menu = buildLayerMenu({ layer, layerCount: 2, hasSelection: true, hideEmpty: true });
    expect(entriesWithoutIcon(menu)).toEqual([]);
    expect(menuItems(menu).filter((i) => toolIcon(i.icon ?? '') === null)).toEqual([]);
    const hide = menuItems(menu).find((i) => i.id === 'hideEmpty');
    expect([hide?.checked, hide?.icon]).toEqual([true, 'layerHideEmpty']);
  });
});

describe('layer list filters and deletion', () => {
  const a = { ...layer, id: 1, count: 3, active: false };
  const b = { ...layer, id: 2, count: 0, active: false };
  const c = { ...layer, id: 3, count: 0, active: true };

  it('hides empty layers but keeps the active one', () => {
    expect(shownLayers([a, b, c], true).map((l) => l.id)).toEqual([1, 3]);
    expect(shownLayers([a, b, c], false).map((l) => l.id)).toEqual([1, 2, 3]);
  });

  it('toggles Hide Empty Layers from the right click menu', () => {
    const items = menuItems(
      buildLayerMenu({ layer: a, layerCount: 2, hasSelection: false, hideEmpty: true }),
    );
    const item = items.find((i) => i.id === 'hideEmpty');
    expect(item?.action).toBe('toggleHideEmpty');
    expect(item?.checked).toBe(true);
  });

  it('offers to delete a layer that still holds objects', () => {
    const items = menuItems(buildLayerMenu({ layer: a, layerCount: 2, hasSelection: false }));
    expect(items.find((i) => i.id === 'delete')?.disabled).toBe(false);
  });
});

describe('layer reordering', () => {
  it('moves a dragged layer onto the row it was dropped on', () => {
    expect(reorderedIds([1, 2, 3, 4], 0, 2)).toEqual([2, 3, 1, 4]);
    expect(reorderedIds([1, 2, 3, 4], 3, 1)).toEqual([1, 4, 2, 3]);
    expect(reorderedIds([1, 2, 3], 1, 1)).toBeNull();
    expect(reorderedIds([1, 2, 3], 2, 3)).toBeNull();
  });

  it('shows icons that exist', () => {
    const items = menuItems(buildLayerMenu({ layer, layerCount: 2, hasSelection: true }));
    expect(items.find((i) => i.id === 'delete')?.icon).toBe('editDelete');
    expect(items.filter((i) => i.icon !== undefined && toolIcon(i.icon) === null)).toEqual([]);
  });
});
