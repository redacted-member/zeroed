import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { setKernelForTests } from '../bridge/client';
import type { PrefsPatch, Preferences } from '../bridge/prefs';
import { fakeKernel } from '../testing/fakeKernel';

import { DEFAULT_PREFS, defaultPrefs, mergePrefs, prefsProblem } from '../lib/prefs';
import {
  WIDTH_SAVE_DELAY_MS,
  applyCssVars,
  confirmationOn,
  usePrefsStore,
  watchLayout,
} from './prefsStore';
import { useUiStore } from './uiStore';
import { useViewStore } from './viewStore';

describe('prefs store', () => {
  // The kernel side of preferences: a document merged patch by patch.
  let stored: Preferences = defaultPrefs();
  beforeAll(() => {
    setKernelForTests(
      fakeKernel((name, args) => {
        if (name === 'prefs.set') {
          const next = mergePrefs(stored, args as PrefsPatch);
          const problem = prefsProblem(next);
          if (problem !== null) {
            throw new Error(problem);
          }
          stored = next;
        } else if (name === 'prefs.reset') {
          stored = defaultPrefs();
        }
        return { ...stored, path: null, setAside: null };
      }),
    );
  });
  afterAll(() => setKernelForTests(null));

  beforeEach(async () => {
    await usePrefsStore.getState().reset();
    usePrefsStore.setState({ loaded: false, prefs: defaultPrefs(), error: null });
  });

  it('loads the kernel preferences and applies the 2D view defaults', async () => {
    await usePrefsStore.getState().update({
      view2d: { snap: { grid: true }, snapRadiusPx: 7, showRulers: false },
    });
    useViewStore.getState().setSnap({ grid: false });
    usePrefsStore.setState({ loaded: false });
    await usePrefsStore.getState().load();
    const state = usePrefsStore.getState();
    expect(state.loaded).toBe(true);
    expect(state.path).toBeNull();
    expect(state.prefs.view2d.snapRadiusPx).toBe(7);
    const view = useViewStore.getState();
    expect(view.snap.grid).toBe(true);
    expect(view.snapRadiusPx).toBe(7);
    expect(view.showRulers).toBe(false);
  });

  it('restores the remembered tab and view layout at load', async () => {
    await usePrefsStore.getState().update({
      window: { activeTab: 'layers', viewMode: 'split' },
    });
    await usePrefsStore.getState().load();
    expect(useUiStore.getState().activeTab).toBe('layers');
    expect(useUiStore.getState().viewMode).toBe('split');
    useUiStore.getState().setActiveTab('design');
    useUiStore.getState().setViewMode('2d');
  });

  it('remembers the Sheets tab', async () => {
    await usePrefsStore.getState().update({ window: { activeTab: 'sheets' } });
    await usePrefsStore.getState().load();
    expect(useUiStore.getState().activeTab).toBe('sheets');
    useUiStore.getState().setActiveTab('design');
  });

  it('opens an older layout that remembered the Toolpaths tab on Design', async () => {
    useUiStore.getState().setActiveTab('clipart');
    await usePrefsStore.getState().update({ window: { activeTab: 'toolpaths' } });
    await usePrefsStore.getState().load();
    expect(useUiStore.getState().activeTab).toBe('design');
  });

  it('keeps a refused update out of the store and reports it', async () => {
    const ok = await usePrefsStore.getState().update({ general: { undoLimit: 0 } });
    expect(ok).toBe(false);
    const state = usePrefsStore.getState();
    expect(state.error).not.toBeNull();
    expect(state.prefs.general.undoLimit).toBe(DEFAULT_PREFS.general.undoLimit);
  });

  it('moves opened jobs to the front of the recent files list', async () => {
    await usePrefsStore.getState().load();
    await usePrefsStore.getState().addRecent('/jobs/a.carve');
    await usePrefsStore.getState().addRecent('/jobs/b.carve');
    await usePrefsStore.getState().addRecent('/jobs/a.carve');
    expect(usePrefsStore.getState().prefs.general.recentFiles).toEqual([
      '/jobs/a.carve',
      '/jobs/b.carve',
    ]);
    await usePrefsStore.getState().clearRecent();
    expect(usePrefsStore.getState().prefs.general.recentFiles).toEqual([]);
  });

  it('answers whether a warning should ask first', async () => {
    expect(confirmationOn('deleteLayer')).toBe(true);
    await usePrefsStore.getState().update({ confirmations: { deleteLayer: false } });
    expect(confirmationOn('deleteLayer')).toBe(false);
    expect(confirmationOn('deleteSheet')).toBe(true);
  });

  it('restores the pin states and dock widths and saves them when they change', async () => {
    vi.useFakeTimers();
    try {
      await usePrefsStore.getState().update({
        window: { toolpathsPinned: false, leftPinned: false, rightWidth: 356, leftWidth: 300 },
      });
      await usePrefsStore.getState().load();
      expect(useUiStore.getState()).toMatchObject({
        toolpathsPinned: false,
        leftPinned: false,
        rightWidth: 356,
        leftWidth: 300,
      });
      const stop = watchLayout();
      useUiStore.getState().toggleToolpathsPinned();
      await vi.advanceTimersByTimeAsync(0);
      expect(usePrefsStore.getState().prefs.window.toolpathsPinned).toBe(true);
      useUiStore.getState().setDockWidth('right', 380);
      useUiStore.getState().setDockWidth('right', 410);
      await vi.advanceTimersByTimeAsync(WIDTH_SAVE_DELAY_MS / 2);
      expect(usePrefsStore.getState().prefs.window.rightWidth).toBe(356);
      await vi.advanceTimersByTimeAsync(WIDTH_SAVE_DELAY_MS);
      expect(usePrefsStore.getState().prefs.window.rightWidth).toBe(410);
      stop();
    } finally {
      vi.useRealTimers();
      useUiStore.setState({
        leftPinned: true,
        toolpathsPinned: true,
        leftWidth: 268,
        rightWidth: 300,
      });
    }
  });

  it('saves layout changes only while remembering is on', async () => {
    await usePrefsStore.getState().load();
    const stop = watchLayout();
    useUiStore.getState().setViewMode('3d');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(usePrefsStore.getState().prefs.window.viewMode).toBe('3d');

    await usePrefsStore.getState().update({ window: { remember: false } });
    useUiStore.getState().setViewMode('split');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(usePrefsStore.getState().prefs.window.viewMode).toBe('3d');
    stop();
    useUiStore.getState().setViewMode('2d');
  });

  it('sets and clears the colour custom properties on a root element', () => {
    const set = new Map<string, string>();
    const root = {
      style: {
        setProperty: (name: string, value: string) => set.set(name, value),
        removeProperty: (name: string) => set.delete(name),
      },
    } as unknown as HTMLElement;
    applyCssVars(
      mergePrefs(DEFAULT_PREFS, { view3d: { background: 'solid', topColour: '#101010' } }),
      root,
    );
    expect(set.get('--view3d-top')).toBe('#101010');
    expect(set.get('--view3d-bottom')).toBe('#101010');
    applyCssVars(DEFAULT_PREFS, root);
    expect(set.has('--view3d-top')).toBe(false);
    expect(set.get('--canvas-toolpath-rapid')).toBe('#d62828');
  });
});
