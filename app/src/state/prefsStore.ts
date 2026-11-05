// The user's preferences, mirrored from the kernel (prefs.get / prefs.set),
// and the side effects that make the rest of the app follow them.

import { create } from 'zustand';

import { api } from '../bridge/api';
import { getKernel } from '../bridge/client';
import type { Preferences, PrefsPatch, PrefsReply, WindowPrefs } from '../bridge/prefs';
import type { Unsubscribe } from '../bridge/types';
import {
  clampDockWidth,
  clampListHeight,
  defaultPrefs,
  dockTab,
  prefsCssVars,
  windowView,
  withRecent,
} from '../lib/prefs';
import { useUiStore } from './uiStore';
import { useViewStore } from './viewStore';

interface PrefsState {
  prefs: Preferences;
  /** True once prefs.get has answered. */
  loaded: boolean;
  /** The file the kernel saves preferences in, null when it does not. */
  path: string | null;
  /** Where a broken preferences file was moved at start, if one was. */
  setAside: string | null;
  /** The last refusal of prefs.set, for the dialog to show. */
  error: string | null;
  dialogOpen: boolean;
  openDialog: () => void;
  closeDialog: () => void;
  /** Reads the preferences and applies them as a session start does. */
  load: () => Promise<void>;
  /** Changes some preferences. Answers false when the kernel refuses. */
  update: (patch: PrefsPatch) => Promise<boolean>;
  /** Puts every preference back to its default. */
  reset: () => Promise<boolean>;
  /** Moves a job to the front of the recent files list. */
  addRecent: (path: string) => Promise<void>;
  clearRecent: () => Promise<void>;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function strip(reply: PrefsReply): Preferences {
  return {
    version: reply.version,
    general: reply.general,
    view2d: reply.view2d,
    view3d: reply.view3d,
    toolpaths: reply.toolpaths,
    confirmations: reply.confirmations,
    window: reply.window,
    machines: reply.machines,
    modeling: reply.modeling ?? { sculptPresets: [] },
  };
}

/** Sets the colour custom properties on the document root. */
export function applyCssVars(prefs: Preferences, root: HTMLElement | null): void {
  if (root === null) {
    return;
  }
  for (const [name, value] of Object.entries(prefsCssVars(prefs))) {
    if (value === null) {
      root.style.removeProperty(name);
    } else {
      root.style.setProperty(name, value);
    }
  }
}

/** Puts the 2D view options a session starts with into the view store. */
export function applyViewDefaults(prefs: Preferences): void {
  const { snap, snapRadiusPx, showGrid, showRulers, gridSpacing, snapDistances, nudgeDistance } =
    prefs.view2d;
  const view = useViewStore.getState();
  view.setSnapDistances(snapDistances);
  view.setNudgeDistance(nudgeDistance);
  view.setSnap(snap);
  view.setSnapRadiusPx(snapRadiusPx);
  view.setGridSpacing(gridSpacing);
  if (view.showGrid !== showGrid) {
    view.toggleGrid();
  }
  if (view.showRulers !== showRulers) {
    view.toggleRulers();
  }
}

function rootElement(): HTMLElement | null {
  return typeof document === 'undefined' ? null : document.documentElement;
}

function sameList(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((value, index) => value === b[index]);
}

/** Applies what changed between `before` and `after`. */
function applyChanges(before: Preferences | null, after: Preferences): void {
  applyCssVars(after, rootElement());
  if (before === null || JSON.stringify(before.view2d) !== JSON.stringify(after.view2d)) {
    applyViewDefaults(after);
  }
  const recent = after.general.recentFiles;
  if (before === null || !sameList(before.general.recentFiles, recent)) {
    void getKernel().setRecentFiles?.(recent);
  }
}

export const usePrefsStore = create<PrefsState>()((set, get) => {
  const accept = (reply: PrefsReply) => {
    const before = get().loaded ? get().prefs : null;
    const prefs = strip(reply);
    set({ prefs, loaded: true, path: reply.path, setAside: reply.setAside, error: null });
    applyChanges(before, prefs);
  };

  const run = async (call: () => Promise<PrefsReply>): Promise<boolean> => {
    try {
      accept(await call());
      return true;
    } catch (error) {
      set({ error: message(error) });
      return false;
    }
  };

  return {
    prefs: defaultPrefs(),
    loaded: false,
    path: null,
    setAside: null,
    error: null,
    dialogOpen: false,
    openDialog: () => set({ dialogOpen: true, error: null }),
    closeDialog: () => set({ dialogOpen: false, error: null }),
    load: async () => {
      if (await run(() => api.prefs.get())) {
        const layout = get().prefs.window;
        if (layout.remember) {
          const ui = useUiStore.getState();
          ui.setActiveTab(dockTab(layout.activeTab));
          ui.setViewMode(windowView(layout.viewMode));
          useUiStore.setState(dockLayout(layout));
        }
      }
    },
    update: (patch) => run(() => api.prefs.set(patch)),
    reset: () => run(() => api.prefs.reset()),
    addRecent: async (path) => {
      const { recentFiles, recentFilesLength } = get().prefs.general;
      const next = withRecent(recentFiles, path, recentFilesLength);
      if (!sameList(next, recentFiles)) {
        await get().update({ general: { recentFiles: next } });
      }
    },
    clearRecent: async () => {
      await get().update({ general: { recentFiles: [] } });
    },
  };
});

/** Whether the warning `kind` should ask before going ahead. */
export function confirmationOn(
  kind: 'deleteLayer' | 'deleteSheet' | 'deleteTool' | 'embossWarning',
): boolean {
  return usePrefsStore.getState().prefs.confirmations[kind];
}

/** The dock state a remembered window layout restores. */
export function dockLayout(layout: WindowPrefs): {
  leftPinned: boolean;
  toolpathsPinned: boolean;
  leftWidth: number;
  rightWidth: number;
  leftOpen: boolean;
  toolpathsOpen: boolean;
  toolpathListHeight: number;
  componentTreeHeight: number;
} {
  return {
    toolpathListHeight: clampListHeight(layout.toolpathListHeight),
    componentTreeHeight: clampListHeight(layout.componentTreeHeight),
    leftPinned: layout.leftPinned,
    toolpathsPinned: layout.toolpathsPinned,
    leftWidth: clampDockWidth(layout.leftWidth),
    rightWidth: clampDockWidth(layout.rightWidth),
    leftOpen: false,
    toolpathsOpen: false,
  };
}

type LayoutKey =
  | 'activeTab'
  | 'viewMode'
  | 'leftPinned'
  | 'toolpathsPinned'
  | 'leftWidth'
  | 'rightWidth'
  | 'toolpathListHeight'
  | 'componentTreeHeight';
const LAYOUT_KEYS: readonly LayoutKey[] = [
  'activeTab',
  'viewMode',
  'leftPinned',
  'toolpathsPinned',
  'leftWidth',
  'rightWidth',
  'toolpathListHeight',
  'componentTreeHeight',
];

/** How long a dock width or list height must stay put before it is saved, in ms. */
export const WIDTH_SAVE_DELAY_MS = 400;

/**
 * Saves the tab, view and dock layout whenever it changes, while the
 * preferences say to remember it. Dock widths are saved once a drag settles.
 * Answers the unsubscribe function.
 */
export function watchLayout(): Unsubscribe {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const save = () => {
    timer = null;
    const { loaded, prefs, update } = usePrefsStore.getState();
    if (!loaded || !prefs.window.remember) {
      return;
    }
    const ui = useUiStore.getState();
    const patch: Partial<Pick<WindowPrefs, LayoutKey>> = {};
    let changed = false;
    for (const key of LAYOUT_KEYS) {
      if (prefs.window[key] !== ui[key]) {
        Object.assign(patch, { [key]: ui[key] });
        changed = true;
      }
    }
    if (changed) {
      void update({ window: patch });
    }
  };
  const stop = useUiStore.subscribe((ui, previous) => {
    if (!LAYOUT_KEYS.some((key) => ui[key] !== previous[key])) {
      return;
    }
    const widthOnly =
      ui.activeTab === previous.activeTab &&
      ui.viewMode === previous.viewMode &&
      ui.leftPinned === previous.leftPinned &&
      ui.toolpathsPinned === previous.toolpathsPinned;
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    if (widthOnly) {
      timer = setTimeout(save, WIDTH_SAVE_DELAY_MS);
    } else {
      save();
    }
  });
  return () => {
    if (timer !== null) {
      clearTimeout(timer);
    }
    stop();
  };
}
