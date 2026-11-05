import type { JSX } from 'react';

import { sendToolCommand } from '../../lib/toolCommands';
import { strings } from '../../lib/strings';
import { runAppCommand } from '../../shell/AppShell/AppShell';
import { useClipboardStore } from '../../state/clipboardStore';
import { useDocumentStore } from '../../state/documentStore';
import { useDrawStore } from '../../state/drawStore';
import { useNodeToolStore } from '../../state/nodeToolStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { PopupMenu } from '../PopupMenu/PopupMenu';
import type { PopupEntry } from '../PopupMenu/model';
import { useExtraIcons } from '../../shell/ToolIcons/ToolIcons';
import { buildMenu, controllerIcon, type MenuAction } from './model';

/** The view's actions, or an entry of the open form's own menu. */
type ViewAction = MenuAction | { controller: string };

/** Right click menu of the drawing view. Rendered only while open. */
export function ContextMenu(): JSX.Element | null {
  const menu = useUiStore((state) => state.contextMenu);
  const closeMenu = useUiStore((state) => state.closeContextMenu);
  const selection = useDocumentStore((state) => state.selection);
  const entities = useDocumentStore((state) => state.entities);
  const undo = useDocumentStore((state) => state.undo);
  const clipboardCount = useClipboardStore((state) => state.count);
  const activeTool = useViewStore((state) => state.activeTool);
  const nodeHover = useNodeToolStore((state) => state.hover);

  // The icons menu commands name load after start up; the render once they
  // are in builds the entries again (built on every render: the menu is
  // small and only built while open).
  useExtraIcons();
  const entries = ((): PopupEntry<ViewAction>[] => {
    if (menu === null) {
      return [];
    }
    if (menu.items !== undefined) {
      return menu.items.map((item) => ({
        id: item.id,
        label: item.label,
        icon: controllerIcon(item.id),
        ...(item.checked === undefined ? {} : { checked: item.checked }),
        ...(item.disabled === undefined ? {} : { disabled: item.disabled }),
        action: { controller: item.id },
      }));
    }
    const selected = selection.ids
      .map((id) => entities.find((e) => e.id === id))
      .filter((e) => e !== undefined);
    const hasGroup = selected.some((e) => e.kind === 'group');
    const hasBitmap = selected.some((e) => e.kind === 'bitmap');
    const hasClosedSelection = selected.some((e) => e.kind !== 'bitmap' && e.closed);
    return buildMenu({
      kind: activeTool === 'node' ? 'node' : 'canvas',
      selectedCount: selection.ids.length,
      clipboardCount,
      canUndo: undo.canUndo,
      canRedo: undo.canRedo,
      undoLabel: undo.undoLabel,
      redoLabel: undo.redoLabel,
      hasGroup,
      hasClosedSelection,
      hasBitmap,
      canCropBitmap: hasBitmap && hasClosedSelection,
      hasCurveText: selected.some((e) => e.onCurve !== undefined),
      hasText: selected.length === 1 && selected[0]?.kind === 'text',
      nodeHover,
    });
  })();

  if (menu === null) {
    return null;
  }

  const run = (action: ViewAction) => {
    if ('controller' in action) {
      useDrawStore.getState().controller?.menuAction?.(action.controller);
    } else if ('toolKey' in action) {
      sendToolCommand(action.toolKey);
    } else {
      void runAppCommand(action.command);
    }
  };

  return (
    <PopupMenu
      entries={entries}
      label={strings.contextMenu.label}
      onClose={closeMenu}
      onSelect={run}
      x={menu.x}
      y={menu.y}
    />
  );
}
