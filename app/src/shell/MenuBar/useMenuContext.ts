// Store glue for the menus: the live context they are built from and the
// runner for a chosen item.

import { useMemo } from 'react';

import { openRecent } from '../../lib/fileCommands';
import { IS_MAC } from '../../lib/keyLabels';
import { isRotary } from '../../lib/rotary';
import { useDocumentStore } from '../../state/documentStore';
import { useEnableState } from '../../state/enableSnapshot';
import { useGadgetStore } from '../../state/gadgetStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { usePreviewStore } from '../../state/previewStore';
import { useViewStore } from '../../state/viewStore';
import { runEnabledCommand } from '../AppShell/AppShell';
import { displayState } from './displayState';
import type { MenuAction, MenuContext } from './model';

/** The state the menus show, kept current by the stores. */
export function useMenuContext(): MenuContext {
  const recentFiles = usePrefsStore((s) => s.prefs.general.recentFiles);
  const showBlock = usePrefsStore((s) => s.prefs.view3d.showBlock);
  const shadedBackground = usePrefsStore((s) => s.prefs.view3d.background === 'gradient');
  const gadgets = useGadgetStore((s) => s.gadgets);
  const gadgetsLoaded = useGadgetStore((s) => s.loaded);
  const gadgetFailure = useGadgetStore((s) => s.failure);
  const guidesVisible = useViewStore((s) => s.showGuides);
  const toolpathsPinned = useUiStore((s) => s.toolpathsPinned);
  const previewQuality = usePreviewStore((s) => s.quality);
  const prefs = usePrefsStore((s) => s.prefs);
  const rotary = useDocumentStore((s) => isRotary(s.job));
  const enable = useEnableState();
  const display = useMemo(() => displayState(prefs, rotary), [prefs, rotary]);
  return {
    recentFiles,
    gadgets,
    gadgetsLoaded,
    gadgetFailure,
    guidesVisible,
    toolpathsPinned,
    previewQuality,
    showBlock,
    shadedBackground,
    display,
    enable,
    mac: IS_MAC,
  };
}

/** Runs the action of a menu item. */
export async function runMenuAction(action: MenuAction): Promise<void> {
  switch (action.type) {
    case 'command':
      await runEnabledCommand(action.command);
      return;
    case 'openRecent':
      await openRecent(action.index);
      return;
    case 'gadget':
      useGadgetStore.getState().choose(action.name);
      useUiStore.getState().openForm('gadget');
      return;
    case 'refreshGadgets':
      await useGadgetStore.getState().load();
      return;
  }
}
