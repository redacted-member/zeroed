import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type DragEvent,
  type JSX,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';

import type { Layer } from '../../bridge/document';
import { menuAnchor } from '../../lib/menuAnchor';
import { strings } from '../../lib/formStrings';
import { moveWhy } from '../../forms/ButtonFace/why';
import { whyTip } from '../../shell/Tooltip/model';
import { useDocumentStore } from '../../state/documentStore';
import { toolIcon, useExtraIcons } from '../../shell/ToolIcons/ToolIcons';
import { confirmationOn } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { IconButton } from '../../forms/IconButton/IconButton';
import { PopupMenu } from '../../views/PopupMenu/PopupMenu';
import { AddLayerDialog } from '../AddLayerDialog/AddLayerDialog';
import { buildLayerMenu, reorderedIds, shownLayers, type LayerAction } from './menu';

import './LayersPanel.scss';

interface LayerRowProps {
  layer: Layer;
  index: number;
  count: number;
  editing: boolean;
  setEditing: (editing: boolean) => void;
  onDragStart: (index: number) => void;
  onDropOn: (index: number) => void;
  onMenu: (event: MouseEvent<HTMLLIElement>) => void;
}

/** Asks before deleting a layer that still holds objects. */
function removeLayer(layer: Layer): void {
  if (
    layer.count > 0 &&
    confirmationOn('deleteLayer') &&
    !window.confirm(strings.layersPanel.deleteConfirm(layer.name, layer.count))
  ) {
    return;
  }
  void useDocumentStore.getState().deleteLayer(layer.id);
}

function LayerRow({
  layer,
  index,
  count,
  editing,
  setEditing,
  onDragStart,
  onDropOn,
  onMenu,
}: LayerRowProps): JSX.Element {
  const updateLayer = useDocumentStore((state) => state.updateLayer);
  const setActiveLayer = useDocumentStore((state) => state.setActiveLayer);
  const duplicateLayer = useDocumentStore((state) => state.duplicateLayer);
  const reorderLayers = useDocumentStore((state) => state.reorderLayers);
  const moveSelectionToLayer = useDocumentStore((state) => state.moveSelectionToLayer);
  const layers = useDocumentStore((state) => state.layers);
  const hasSelection = useDocumentStore((state) => state.selection.ids.length > 0);
  const [draft, setDraft] = useState(layer.name);
  const [dropTarget, setDropTarget] = useState(false);
  const nameInputRef = useRef<HTMLInputElement | null>(null);
  const moveSelectionWhy = hasSelection ? undefined : strings.why.pickVectors;
  const upWhy = moveWhy(index, count, -1);
  const downWhy = moveWhy(index, count, 1);
  const deleteWhy = count <= 1 ? strings.why.keepOneLayer : undefined;

  useEffect(() => {
    if (editing) {
      nameInputRef.current?.focus();
      nameInputRef.current?.select();
    }
  }, [editing]);

  const commitName = () => {
    setEditing(false);
    const name = draft.trim();
    if (name !== '' && name !== layer.name) {
      void updateLayer(layer.id, { name });
    } else {
      setDraft(layer.name);
    }
  };

  const onNameKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      commitName();
    } else if (event.key === 'Escape') {
      setDraft(layer.name);
      setEditing(false);
    }
  };

  const move = (delta: number) => {
    const ids = reorderedIds(
      layers.map((l) => l.id),
      index,
      index + delta,
    );
    if (ids !== null) {
      void reorderLayers(ids);
    }
  };

  const remove = () => removeLayer(layer);

  const onDragOver = (event: DragEvent<HTMLLIElement>) => {
    event.preventDefault();
    setDropTarget(true);
  };

  const onDrop = (event: DragEvent<HTMLLIElement>) => {
    event.preventDefault();
    setDropTarget(false);
    onDropOn(index);
  };

  const rowClass = [
    'layers-panel__row',
    layer.active ? 'layers-panel__row--active' : '',
    layer.count === 0 ? 'layers-panel__row--empty' : '',
    dropTarget ? 'layers-panel__row--drop' : '',
  ]
    .filter((c) => c !== '')
    .join(' ');

  return (
    <li
      className={rowClass}
      data-layer-id={layer.id}
      draggable
      onDragLeave={() => setDropTarget(false)}
      onDragOver={onDragOver}
      onContextMenu={onMenu}
      onDragStart={() => onDragStart(index)}
      onDrop={onDrop}
    >
      <span
        aria-hidden="true"
        className="layers-panel__grip"
        data-tip={strings.layersPanel.dragHandle}
      >
        {'⠇'}
      </span>
      <input
        aria-label={strings.layersPanel.colour}
        className="layers-panel__colour"
        onChange={(event) => void updateLayer(layer.id, { colour: event.target.value })}
        type="color"
        value={layer.colour}
      />
      <button
        aria-label={layer.visible ? strings.layersPanel.visible : strings.layersPanel.hidden}
        aria-pressed={layer.visible}
        className="layers-panel__toggle"
        onClick={() => void updateLayer(layer.id, { visible: !layer.visible })}
        data-tip={layer.visible ? strings.layersPanel.visible : strings.layersPanel.hidden}
        type="button"
      >
        {toolIcon(layer.visible ? 'visible' : 'hidden')}
      </button>
      <button
        aria-label={layer.locked ? strings.layersPanel.locked : strings.layersPanel.unlocked}
        aria-pressed={layer.locked}
        className="layers-panel__toggle"
        onClick={() => void updateLayer(layer.id, { locked: !layer.locked })}
        data-tip={layer.locked ? strings.layersPanel.locked : strings.layersPanel.unlocked}
        type="button"
      >
        {toolIcon(layer.locked ? 'lock' : 'unlock')}
      </button>
      {editing ? (
        <input
          aria-label={strings.layersPanel.rename}
          className="layers-panel__name-input"
          onBlur={commitName}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={onNameKey}
          ref={nameInputRef}
          type="text"
          value={draft}
        />
      ) : (
        <button
          aria-current={layer.active ? 'true' : undefined}
          className="layers-panel__name"
          onClick={() => void setActiveLayer(layer.id)}
          onDoubleClick={() => {
            setDraft(layer.name);
            setEditing(true);
          }}
          data-tip={strings.layersPanel.setActive}
          type="button"
        >
          <span className="layers-panel__name-text">{layer.name}</span>
          {layer.keepOut === true ? (
            <span
              className="layers-panel__badge"
              data-tip={`${strings.layersPanel.keepOutHint} ${strings.layersPanel.objects(layer.count)}`}
            >
              {strings.layersPanel.keepOut}
            </span>
          ) : (
            <span className="layers-panel__count">
              {layer.count === 0
                ? strings.layersPanel.empty
                : strings.layersPanel.objects(layer.count)}
            </span>
          )}
        </button>
      )}
      <span className="layers-panel__actions">
        <button
          aria-label={strings.layersPanel.moveSelection}
          className="layers-panel__action"
          disabled={moveSelectionWhy !== undefined}
          onClick={() => void moveSelectionToLayer(layer.id)}
          data-tip={whyTip(strings.layersPanel.moveSelection, moveSelectionWhy)}
          type="button"
        >
          {toolIcon('moveToLayer')}
        </button>
        <button
          aria-label={strings.layersPanel.duplicate}
          className="layers-panel__action"
          onClick={() => void duplicateLayer(layer.id)}
          data-tip={strings.layersPanel.duplicate}
          type="button"
        >
          {toolIcon('editCopy')}
        </button>
        <button
          aria-label={strings.layersPanel.moveUp}
          className="layers-panel__action"
          disabled={upWhy !== undefined}
          onClick={() => move(-1)}
          data-tip={whyTip(strings.layersPanel.moveUp, upWhy)}
          type="button"
        >
          {toolIcon('arrowUp')}
        </button>
        <button
          aria-label={strings.layersPanel.moveDown}
          className="layers-panel__action"
          disabled={downWhy !== undefined}
          onClick={() => move(1)}
          data-tip={whyTip(strings.layersPanel.moveDown, downWhy)}
          type="button"
        >
          {toolIcon('arrowDown')}
        </button>
        <button
          aria-label={strings.layersPanel.delete}
          className="layers-panel__action"
          disabled={deleteWhy !== undefined}
          onClick={remove}
          data-tip={whyTip(strings.layersPanel.delete, deleteWhy)}
          type="button"
        >
          {toolIcon('editDelete')}
        </button>
      </span>
    </li>
  );
}

/** The Layers list: colour, visibility, lock, name, order and membership. */
export function LayersPanel(): JSX.Element {
  // Menu, form and panel icons load after start up; redraw once they are in.
  useExtraIcons();
  const layers = useDocumentStore((state) => state.layers);
  const updateLayer = useDocumentStore((state) => state.updateLayer);
  const reorderLayers = useDocumentStore((state) => state.reorderLayers);
  const hideEmpty = useUiStore((state) => state.hideEmptyLayers);
  const toggleHideEmpty = useUiStore((state) => state.toggleHideEmptyLayers);
  const dragIndexRef = useRef<number | null>(null);
  const [renamingId, setRenamingId] = useState<number | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; id: number } | null>(null);
  /** The Add New Layer dialog: null closed, else the layer to insert before (if any). */
  const [adding, setAdding] = useState<{ before?: number } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const menuLayer = menu === null ? null : (layers.find((l) => l.id === menu.id) ?? null);
  const hasSelection = useDocumentStore((state) => state.selection.ids.length > 0);

  const runMenu = (action: LayerAction) => {
    if (menuLayer === null) {
      return;
    }
    const doc = useDocumentStore.getState();
    const id = menuLayer.id;
    switch (action) {
      case 'rename':
        setRenamingId(id);
        break;
      case 'toggleVisible':
        void doc.updateLayer(id, { visible: !menuLayer.visible });
        break;
      case 'toggleLocked':
        void doc.updateLayer(id, { locked: !menuLayer.locked });
        break;
      case 'toggleKeepOut':
        void doc.updateLayer(id, { keepOut: menuLayer.keepOut !== true });
        break;
      case 'makeActive':
        void doc.setActiveLayer(id);
        break;
      case 'duplicate':
        void doc.duplicateLayer(id);
        break;
      case 'delete':
        removeLayer(menuLayer);
        break;
      case 'selectAll':
        void doc.select(doc.entities.filter((e) => e.layer === id).map((e) => e.id));
        break;
      case 'moveSelection':
        void doc.moveSelectionToLayer(id);
        break;
      case 'newLayer':
        setAdding({});
        break;
      case 'insertLayer':
        setAdding({ before: id });
        break;
      case 'mergeIntoActive': {
        const active = doc.layers.find((l) => l.active);
        if (active !== undefined && active.id !== id) {
          void doc.deleteLayer(id, active.id);
        }
        break;
      }
      case 'toggleHideEmpty':
        toggleHideEmpty();
        break;
    }
  };

  const setAll = async (visible: boolean) => {
    for (const layer of layers) {
      if (layer.visible !== visible) {
        await updateLayer(layer.id, { visible });
      }
    }
  };

  const onDropOn = (targetIndex: number) => {
    const from = dragIndexRef.current;
    dragIndexRef.current = null;
    const ids =
      from === null
        ? null
        : reorderedIds(
            layers.map((l) => l.id),
            from,
            targetIndex,
          );
    if (ids !== null) {
      void reorderLayers(ids);
    }
  };

  const shown = shownLayers(layers, hideEmpty);

  return (
    <div className="layers-panel">
      <div className="layers-panel__toolbar">
        <IconButton
          icon="layerAdd"
          label={strings.layersPanel.short.add}
          onClick={() => setAdding({})}
          title={strings.layersPanel.add}
        />
        <IconButton
          icon="visible"
          label={strings.layersPanel.short.showAll}
          onClick={() => void setAll(true)}
          title={strings.layersPanel.showAllLayers}
        />
        <IconButton
          icon="hidden"
          label={strings.layersPanel.short.hideAll}
          onClick={() => void setAll(false)}
          title={strings.layersPanel.hideAllLayers}
        />
        <IconButton
          icon="layerHideEmpty"
          label={strings.layersPanel.short.hideEmpty}
          onClick={() => toggleHideEmpty()}
          pressed={hideEmpty}
          title={strings.layersPanel.hideEmpty}
        />
      </div>
      <ul className="layers-panel__list">
        {shown.map((layer) => (
          <LayerRow
            count={layers.length}
            index={layers.findIndex((l) => l.id === layer.id)}
            key={layer.id}
            editing={renamingId === layer.id}
            layer={layer}
            onDragStart={(index) => {
              dragIndexRef.current = index;
            }}
            onDropOn={onDropOn}
            onMenu={(event) => {
              event.preventDefault();
              setMenu({ ...menuAnchor(event), id: layer.id });
            }}
            setEditing={(editing) => setRenamingId(editing ? layer.id : null)}
          />
        ))}
      </ul>
      {adding === null ? null : (
        <AddLayerDialog layers={layers} onClose={() => setAdding(null)} {...adding} />
      )}
      {menu === null || menuLayer === null ? null : (
        <PopupMenu
          entries={buildLayerMenu({
            layer: menuLayer,
            layerCount: layers.length,
            hasSelection,
            hideEmpty,
          })}
          label={strings.panelMenus.layerLabel(menuLayer.name)}
          onClose={closeMenu}
          onSelect={runMenu}
          x={menu.x}
          y={menu.y}
        />
      )}
    </div>
  );
}
