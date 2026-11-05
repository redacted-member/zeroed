// The native menu of the desktop build is the same tree as the in-window
// menu bar: the page builds the menus from the stores (model.ts), turns
// them into the plain shape app/src-tauri/src/menu.rs builds a menu bar
// from, and sends it again whenever what they show changes (recent files,
// gadgets, check marks, enabled items).

import { getKernel } from '../../bridge';
import type { Unsubscribe } from '../../bridge/types';
import { editsFields } from '../../lib/fieldEditing';
import { SHORTCUTS, type AppCommand } from '../../lib/shortcuts';
import { isRotary } from '../../lib/rotary';
import { useClipboardStore } from '../../state/clipboardStore';
import { useDocumentStore } from '../../state/documentStore';
import { currentEnableState } from '../../state/enableSnapshot';
import { useModelStore } from '../../state/modelStore';
import { useGadgetStore } from '../../state/gadgetStore';
import { usePrefsStore } from '../../state/prefsStore';
import { usePreviewStore } from '../../state/previewStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { displayState } from './displayState';
import { loadExtraIcons } from '../ToolIcons/ToolIcons';
import { buildMenus, type MenuContext, type MenuEntry, type MenuSpec } from './model';

export type NativeEntry =
  | {
      type: 'item';
      id: string;
      label: string;
      accelerator: string | null;
      enabled: boolean;
      /** Set for check and radio items. */
      checked: boolean | null;
      /** The icon drawn beside the label (menuIcons.ts). */
      icon: string | null;
    }
  | { type: 'separator' }
  | { type: 'submenu'; label: string; entries: NativeEntry[]; icon: string | null }
  | { type: 'predefined'; name: 'cut' | 'copy' | 'paste'; icon: string | null };

export interface NativeMenu {
  label: string;
  entries: NativeEntry[];
}

/** Items the macOS app menu holds instead (Settings, About). */
const APP_MENU_ITEMS: ReadonlySet<string> = new Set(['app.preferences', 'help.about']);

const PREDEFINED: Readonly<Record<string, 'cut' | 'copy' | 'paste'>> = {
  'edit.cut': 'cut',
  'edit.copy': 'copy',
  'edit.paste': 'paste',
};

/**
 * The native accelerator of a command: only keys with Cmd, function keys,
 * Page Up/Down and Delete, so plain letters keep reaching text fields and
 * the 2D view.
 */
export function acceleratorFor(command: AppCommand): string | null {
  const shortcut = SHORTCUTS.find((s) => s.command === command);
  if (shortcut === undefined) {
    return null;
  }
  const key = shortcut.key.length === 1 ? shortcut.key.toUpperCase() : shortcut.key;
  if (shortcut.ctrl === true) {
    const mods = [
      'CmdOrCtrl',
      shortcut.shift === true ? 'Shift' : null,
      shortcut.alt === true ? 'Alt' : null,
    ];
    return [...mods.filter((m) => m !== null), key].join('+');
  }
  if (/^F\d+$/.test(key) || key === 'PageUp' || key === 'PageDown' || key === 'Delete') {
    return key;
  }
  return null;
}

/** Drops leading, trailing and doubled separators. */
function tidy(entries: NativeEntry[]): NativeEntry[] {
  const out: NativeEntry[] = [];
  for (const entry of entries) {
    if (
      entry.type === 'separator' &&
      (out.length === 0 || out[out.length - 1]?.type === 'separator')
    ) {
      continue;
    }
    out.push(entry);
  }
  while (out[out.length - 1]?.type === 'separator') {
    out.pop();
  }
  return out;
}

function nativeEntries(entries: readonly MenuEntry[]): NativeEntry[] {
  const out: NativeEntry[] = [];
  for (const entry of entries) {
    if (entry.kind === 'separator') {
      out.push({ type: 'separator' });
    } else if (entry.kind === 'submenu') {
      out.push({
        type: 'submenu',
        label: entry.label,
        entries: nativeEntries(entry.entries),
        icon: entry.icon ?? null,
      });
    } else if (!APP_MENU_ITEMS.has(entry.id)) {
      const predefined = PREDEFINED[entry.id];
      if (predefined !== undefined) {
        out.push({ type: 'predefined', name: predefined, icon: entry.icon ?? null });
        continue;
      }
      out.push({
        type: 'item',
        id: entry.id,
        label: entry.label,
        accelerator: entry.action.type === 'command' ? acceleratorFor(entry.action.command) : null,
        // Undo, Redo and Delete stay live natively: their accelerators edit a
        // focused text box, and runAppCommand ignores them when greyed.
        enabled:
          entry.disabled !== true ||
          (entry.action.type === 'command' && editsFields(entry.action.command)),
        checked: entry.checked ?? null,
        icon: entry.icon ?? null,
      });
    }
  }
  return tidy(out);
}

/** The menus in the shape the native shell builds. */
export function nativeMenus(menus: readonly MenuSpec[]): NativeMenu[] {
  return menus.map((menu) => ({ label: menu.label, entries: nativeEntries(menu.entries) }));
}

/** Every icon the menus name, each once. */
export function menuIconIds(menus: readonly NativeMenu[]): string[] {
  const ids = new Set<string>();
  const visit = (entries: readonly NativeEntry[]) => {
    for (const entry of entries) {
      if (entry.type !== 'separator' && entry.icon !== null) {
        ids.add(entry.icon);
      }
      if (entry.type === 'submenu') {
        visit(entry.entries);
      }
    }
  };
  for (const menu of menus) {
    visit(menu.entries);
  }
  return [...ids];
}

/** The menu context from the stores, outside React. */
export function currentMenuContext(): MenuContext {
  const prefs = usePrefsStore.getState().prefs;
  const gadgets = useGadgetStore.getState();
  return {
    recentFiles: prefs.general.recentFiles,
    gadgets: gadgets.gadgets,
    gadgetsLoaded: gadgets.loaded,
    gadgetFailure: gadgets.failure,
    guidesVisible: useViewStore.getState().showGuides,
    toolpathsPinned: useUiStore.getState().toolpathsPinned,
    previewQuality: usePreviewStore.getState().quality,
    showBlock: prefs.view3d.showBlock,
    shadedBackground: prefs.view3d.background === 'gradient',
    display: displayState(prefs, isRotary(useDocumentStore.getState().job)),
    enable: currentEnableState(),
    mac: true,
  };
}

/**
 * Sends the menus to the native menu bar and again whenever they change.
 * Picks of gadget items go to `runGadget`. Does nothing in a host without
 * a native menu. Answers the function that stops it.
 */
export function watchNativeMenu(runGadget: (name: string) => void): Unsubscribe {
  const kernel = getKernel();
  const setMenus = kernel.setMenus;
  if (setMenus === undefined) {
    return () => {};
  }
  let sent = '';
  let queued = false;
  // Icons are rendered in color the first time a menu names them, and again
  // when the appearance turns light or dark, and reach the shell before the
  // menus that draw them.
  const iconsSent = new Set<string>();
  let chain: Promise<void> = Promise.resolve();
  const sendIcons = async (ids: string[]) => {
    const setMenuIcons = kernel.setMenuIcons;
    if (setMenuIcons === undefined) {
      return;
    }
    try {
      const { iconImages } = await import('./iconBitmaps');
      await setMenuIcons(await iconImages(ids));
    } catch (error) {
      console.error('the menu icons could not be drawn', error);
    }
  };
  const push = () => {
    if (queued) {
      return;
    }
    queued = true;
    queueMicrotask(() => {
      queued = false;
      const menus = nativeMenus(buildMenus(currentMenuContext()));
      const text = JSON.stringify(menus);
      if (text === sent) {
        return;
      }
      sent = text;
      const missing = menuIconIds(menus).filter((id) => !iconsSent.has(id));
      for (const id of missing) {
        iconsSent.add(id);
      }
      chain = chain.then(async () => {
        if (missing.length > 0) {
          await sendIcons(missing);
        }
        await setMenus(menus);
      });
    });
  };
  // Only the fields the menus show; the cursor and camera move constantly.
  const stops = [
    usePrefsStore.subscribe((s, p) => s.prefs !== p.prefs && push()),
    useGadgetStore.subscribe(push),
    useDocumentStore.subscribe(
      (s, p) => (s.job !== p.job || s.selection !== p.selection || s.undo !== p.undo) && push(),
    ),
    useModelStore.subscribe(
      (s, p) => (s.selectedId !== p.selectedId || s.picked !== p.picked) && push(),
    ),
    useClipboardStore.subscribe(push),
    useViewStore.subscribe((s, p) => s.showGuides !== p.showGuides && push()),
    useUiStore.subscribe((s, p) => s.toolpathsPinned !== p.toolpathsPinned && push()),
    usePreviewStore.subscribe((s, p) => s.quality !== p.quality && push()),
    useToolpathStore.subscribe((s, p) => s.selectedId !== p.selectedId && push()),
  ];
  if (!useGadgetStore.getState().loaded) {
    void useGadgetStore.getState().load();
  }
  push();
  // The icons menu commands name load after start up: send the menus again.
  void loadExtraIcons().then(push);
  // The icons carry the colors of the appearance: draw them all again, and
  // send the menus that show them, when it turns light or dark.
  const scheme =
    typeof window.matchMedia === 'function'
      ? window.matchMedia('(prefers-color-scheme: dark)')
      : null;
  const recolor = () => {
    iconsSent.clear();
    sent = '';
    push();
  };
  scheme?.addEventListener('change', recolor);
  let cancelled = false;
  let stopPicks: Unsubscribe | null = null;
  void kernel.onMenuGadget?.(runGadget).then((off) => {
    if (cancelled) {
      off();
    } else {
      stopPicks = off;
    }
  });
  return () => {
    cancelled = true;
    scheme?.removeEventListener('change', recolor);
    for (const stop of stops) {
      stop();
    }
    stopPicks?.();
  };
}
