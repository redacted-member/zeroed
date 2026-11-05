import { useCallback, useEffect, type JSX } from 'react';

import { strings } from '../../lib/strings';
import { useGadgetStore } from '../../state/gadgetStore';
import { useUiStore } from '../../state/uiStore';
import { PopupMenu } from '../../views/PopupMenu/PopupMenu';
import type { PopupEntry } from '../../views/PopupMenu/model';
import { gadgetEntries, type MenuAction } from '../MenuBar/model';
import { runMenuAction, useMenuContext } from '../MenuBar/useMenuContext';

/** Where the popup opens: the top left of the window, under the native menu bar. */
const ANCHOR = { x: 12, y: 8 };

/**
 * The gadgets list as a popup, for Gadgets > Browse Gadgets in the native
 * menu, which cannot list the gadgets itself.
 */
export function GadgetsMenu(): JSX.Element | null {
  const open = useUiStore((store) => store.gadgetsMenuOpen);
  const close = useUiStore((store) => store.closeGadgetsMenu);
  const loaded = useGadgetStore((store) => store.loaded);
  const load = useGadgetStore((store) => store.load);
  const ctx = useMenuContext();
  const onClose = useCallback(() => close(), [close]);

  useEffect(() => {
    if (open && !loaded) {
      void load();
    }
  }, [open, loaded, load]);

  if (!open) {
    return null;
  }
  const entries: PopupEntry<MenuAction>[] = gadgetEntries(ctx).flatMap(
    (entry): PopupEntry<MenuAction>[] =>
      entry.kind === 'separator'
        ? [{ separator: true }]
        : entry.kind === 'item'
          ? [
              {
                id: entry.id,
                label: entry.note === undefined ? entry.label : `${entry.label} (${entry.note})`,
                disabled: entry.disabled,
                icon: entry.icon,
                info: entry.info,
                action: entry.action,
              },
            ]
          : [],
  );
  return (
    <PopupMenu
      entries={entries}
      label={strings.menuBar.gadgets}
      onClose={onClose}
      onSelect={(action) => void runMenuAction(action)}
      x={ANCHOR.x}
      y={ANCHOR.y}
    />
  );
}
