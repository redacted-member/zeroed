// Right click menu of a layer row. Pure so the contents can be tested.

import type { Layer } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { withIcons, type PopupEntry } from '../../views/PopupMenu/model';

/** The icons of the layer menu, by item id. */
const ICONS: Readonly<Record<string, string>> = {
  rename: 'rename',
  visible: 'visible',
  locked: 'lock',
  selectAll: 'selectAll',
  duplicate: 'editCopy',
  delete: 'editDelete',
  newLayer: 'layers',
  keepOut: 'keepOutZones',
  active: 'layerActive',
  moveSelection: 'moveToLayer',
  merge: 'layerMerge',
  insertLayer: 'layerInsert',
  hideEmpty: 'layerHideEmpty',
};

export type LayerAction =
  | 'rename'
  | 'toggleVisible'
  | 'toggleLocked'
  | 'toggleKeepOut'
  | 'makeActive'
  | 'duplicate'
  | 'delete'
  | 'selectAll'
  | 'moveSelection'
  | 'newLayer'
  | 'insertLayer'
  | 'mergeIntoActive'
  | 'toggleHideEmpty';

export interface LayerMenuContext {
  layer: Layer;
  layerCount: number;
  hasSelection: boolean;
  /** Whether empty layers are filtered out of the list. */
  hideEmpty?: boolean;
}

const m = strings.panelMenus;

export function buildLayerMenu({
  layer,
  layerCount,
  hasSelection,
  hideEmpty = false,
}: LayerMenuContext): PopupEntry<LayerAction>[] {
  return withIcons<LayerAction>(
    [
      { id: 'rename', label: m.rename, action: 'rename' },
      { id: 'visible', label: layer.visible ? m.hide : m.show, action: 'toggleVisible' },
      { id: 'locked', label: layer.locked ? m.unlock : m.lock, action: 'toggleLocked' },
      {
        id: 'keepOut',
        label: layer.keepOut === true ? m.clearKeepOut : m.setKeepOut,
        action: 'toggleKeepOut',
      },
      { id: 'active', label: m.makeActive, disabled: layer.active, action: 'makeActive' },
      { separator: true },
      {
        id: 'selectAll',
        label: m.selectAllOnLayer,
        disabled: layer.count === 0 || !layer.visible || layer.locked,
        action: 'selectAll',
      },
      {
        id: 'moveSelection',
        label: m.moveSelectionHere,
        disabled: !hasSelection || layer.locked,
        action: 'moveSelection',
      },
      { separator: true },
      { id: 'duplicate', label: m.duplicate, action: 'duplicate' },
      {
        id: 'merge',
        label: m.mergeIntoActive,
        disabled: layer.active || layer.locked,
        action: 'mergeIntoActive',
      },
      { id: 'delete', label: m.delete, disabled: layerCount <= 1, action: 'delete' },
      { separator: true },
      { id: 'newLayer', label: m.newLayer, action: 'newLayer' },
      { id: 'insertLayer', label: m.insertLayer, action: 'insertLayer' },
      {
        id: 'hideEmpty',
        label: strings.layersPanel.hideEmpty,
        checked: hideEmpty,
        action: 'toggleHideEmpty',
      },
    ],
    ICONS,
  );
}

/** Layers the list shows: with Hide Empty Layers on, only those holding objects and the active one. */
export function shownLayers(layers: readonly Layer[], hideEmpty: boolean): Layer[] {
  return hideEmpty ? layers.filter((l) => l.count > 0 || l.active) : [...layers];
}

/** The layer order after dragging the layer at `from` onto the row at `to`. */
export function reorderedIds(ids: readonly number[], from: number, to: number): number[] | null {
  if (from === to || from < 0 || to < 0 || from >= ids.length || to >= ids.length) {
    return null;
  }
  const next = [...ids];
  const [id] = next.splice(from, 1);
  if (id === undefined) {
    return null;
  }
  next.splice(to, 0, id);
  return next;
}
