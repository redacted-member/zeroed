import { describe, expect, it } from 'vitest';

import {
  dockTab,
  DEFAULT_PREFS,
  defaultPrefs,
  isColour,
  mergePrefs,
  prefsCssVars,
  prefsProblem,
  windowView,
  withRecent,
} from './prefs';

describe('preferences helpers', () => {
  it('the defaults are acceptable and copies are independent', () => {
    expect(prefsProblem(DEFAULT_PREFS)).toBeNull();
    const copy = defaultPrefs();
    copy.view2d.snap.grid = true;
    expect(DEFAULT_PREFS.view2d.snap.grid).toBe(false);
  });

  it('merges a patch group by group and snap field by field', () => {
    const merged = mergePrefs(DEFAULT_PREFS, {
      general: { undoLimit: 50 },
      view2d: { snap: { grid: true } },
    });
    expect(merged.general.undoLimit).toBe(50);
    expect(merged.general.showToolTips).toBe(true);
    expect(merged.view2d.snap.grid).toBe(true);
    expect(merged.view2d.snap.nodes).toBe(true);
    expect(merged.view3d).toEqual(DEFAULT_PREFS.view3d);
  });

  it('keeps the recent files list newest first without duplicates', () => {
    expect(withRecent(['/a', '/b', '/c'], '/b', 10)).toEqual(['/b', '/a', '/c']);
    expect(withRecent(['/a', '/b'], '/c', 2)).toEqual(['/c', '/a']);
    expect(withRecent(['/a'], '/b', 0)).toEqual([]);
  });

  it('names the first value out of range', () => {
    expect(prefsProblem(mergePrefs(DEFAULT_PREFS, { general: { undoLimit: 0 } }))).toBe(
      'general.undoLimit',
    );
    expect(prefsProblem(mergePrefs(DEFAULT_PREFS, { view3d: { topColour: 'red' } }))).toBe(
      'view3d.topColour',
    );
    expect(prefsProblem(mergePrefs(DEFAULT_PREFS, { toolpaths: { tolerance: 2 } }))).toBe(
      'toolpaths.tolerance',
    );
    expect(prefsProblem(mergePrefs(DEFAULT_PREFS, { toolpaths: { geometryFixSeconds: -1 } }))).toBe(
      'toolpaths.geometryFixSeconds',
    );
    expect(
      prefsProblem(mergePrefs(DEFAULT_PREFS, { toolpaths: { solidPreviewColour: 'blue' } })),
    ).toBe('toolpaths.solidPreviewColour');
    expect(isColour('#A0b1C2')).toBe(true);
    expect(isColour('#abc')).toBe(false);
  });

  it('turns background and toolpath colours into custom properties', () => {
    const theme = prefsCssVars(DEFAULT_PREFS);
    expect(theme['--view3d-top']).toBeNull();
    expect(theme['--view3d-bottom']).toBeNull();
    expect(theme['--canvas-toolpath-cut']).toBe('#1d4ed8');
    expect(theme['--canvas-toolpath-solid']).toBe('#5b8def');

    const gradient = prefsCssVars(
      mergePrefs(DEFAULT_PREFS, {
        view3d: { background: 'gradient', topColour: '#111111', bottomColour: '#222222' },
      }),
    );
    expect(gradient['--view3d-top']).toBe('#111111');
    expect(gradient['--view3d-bottom']).toBe('#222222');

    const solid = prefsCssVars(
      mergePrefs(DEFAULT_PREFS, { view3d: { background: 'solid', topColour: '#333333' } }),
    );
    expect(solid['--view3d-bottom']).toBe('#333333');
  });
});

describe('dockTab', () => {
  it('keeps the left dock tabs and maps the older tool tabs to Design', () => {
    expect(dockTab('design')).toBe('design');
    expect(dockTab('components')).toBe('components');
    expect(dockTab('layers')).toBe('layers');
    expect(dockTab('clipart')).toBe('clipart');
    expect(dockTab('drawing')).toBe('design');
    expect(dockTab('modeling')).toBe('design');
    expect(dockTab('toolpaths')).toBe('design');
  });

  it('opens an old Assembly tab or view layout on the Design tab and the 3D view', () => {
    expect(dockTab('assembly')).toBe('design');
    expect(windowView('assembly')).toBe('3d');
    expect(windowView('split')).toBe('split');
  });
});
