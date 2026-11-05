import { beforeAll, describe, expect, it } from 'vitest';

import { IDLE_STATE } from '../../lib/enableState';
import { defaultPrefs } from '../../lib/prefs';
import { displayState } from './displayState';
import { loadExtraIcons } from '../ToolIcons/ToolIcons';
import { buildMenus, type MenuContext } from './model';
import {
  acceleratorFor,
  menuIconIds,
  nativeMenus,
  type NativeEntry,
  type NativeMenu,
} from './nativeMenu';

const CTX: MenuContext = {
  recentFiles: ['/jobs/sign.carve'],
  gadgets: [{ name: 'gear', title: 'Gear Maker', description: '', origin: 'bundled' }],
  gadgetsLoaded: true,
  gadgetFailure: null,
  guidesVisible: false,
  toolpathsPinned: true,
  previewQuality: 'standard',
  showBlock: true,
  shadedBackground: false,
  display: displayState(defaultPrefs(), false),
  enable: { ...IDLE_STATE, native: true },
  mac: true,
};

function items(entries: readonly NativeEntry[]): NativeEntry[] {
  return entries.flatMap((e) => (e.type === 'submenu' ? items(e.entries) : [e]));
}

// The icons menu commands name load lazily in the app.
beforeAll(loadExtraIcons);

describe('nativeMenus', () => {
  const menus = nativeMenus(buildMenus(CTX));
  const all = menus.flatMap((m) => items(m.entries));
  const byId = (id: string) => all.find((e) => e.type === 'item' && e.id === id);

  it('keeps the menus and their order', () => {
    expect(menus.map((m) => m.label)).toEqual([
      'File',
      'Edit',
      'Model',
      'Machine',
      'Toolpaths',
      'View',
      'Gadgets',
      'Help',
    ]);
  });

  it('moves Options and About to the app menu and tidies the separators', () => {
    expect(byId('app.preferences')).toBeUndefined();
    expect(byId('help.about')).toBeUndefined();
    const help = menus[7]?.entries ?? [];
    expect(help[help.length - 1]?.type).not.toBe('separator');
  });

  it('keeps cut, copy and paste native', () => {
    expect(
      all.filter((e) => e.type === 'predefined').map((e) => e.type === 'predefined' && e.name),
    ).toEqual(['cut', 'copy', 'paste']);
  });

  it('carries accelerators, check marks, enabled state, recent files and gadgets', () => {
    expect(byId('edit.undo')).toMatchObject({
      accelerator: 'CmdOrCtrl+Z',
      enabled: true,
      checked: null,
    });
    expect(byId('view.guidesVisible')).toMatchObject({ checked: false });
    expect(byId('edit.swapSides')).toMatchObject({ enabled: false });
    expect(byId('file.recent.0')).toMatchObject({ label: 'sign.carve' });
    expect(byId('gadget.gear')).toMatchObject({ label: 'Gear Maker', enabled: true });
  });
});

describe('acceleratorFor', () => {
  it('gives Cmd, function and paging keys only', () => {
    expect(acceleratorFor('edit.redo')).toBe('CmdOrCtrl+Y');
    expect(acceleratorFor('view.zoomToJob')).toBe('F6');
    expect(acceleratorFor('view.tileVertical')).toBe('PageUp');
    expect(acceleratorFor('edit.delete')).toBe('Delete');
    expect(acceleratorFor('edit.group')).toBeNull();
    expect(acceleratorFor('file.saveAs')).toBeNull();
  });
});

describe('native menu icons', () => {
  // Built once the lazy icons are in (beforeAll), as the app sends them again then.
  let menus: NativeMenu[] = [];
  let all: NativeEntry[] = [];
  beforeAll(() => {
    menus = nativeMenus(buildMenus(CTX));
    all = menus.flatMap((m) => items(m.entries));
  });

  it('passes each item and submenu its icon, check items too', () => {
    const open = all.find((e) => e.type === 'item' && e.id === 'file.open');
    expect(open?.type === 'item' ? open.icon : null).toBe('fileOpen');
    const recent = all.find((e) => e.type === 'item' && e.id === 'file.recent.0');
    expect(recent?.type === 'item' ? recent.icon : null).toBe('fileRecent');
    const importMenu = menus[0]?.entries.find((e) => e.type === 'submenu' && e.label === 'Import');
    expect(importMenu?.type === 'submenu' ? importMenu.icon : null).toBe('importVectors');
    expect(all.filter((e) => e.type === 'item' && e.icon === null)).toEqual([]);
    expect(all.filter((e) => e.type === 'submenu' && e.icon === null)).toEqual([]);
    const guides = all.find((e) => e.type === 'item' && e.id === 'view.guidesVisible');
    expect(guides).toMatchObject({ checked: expect.any(Boolean), icon: 'guidesVisible' });
  });

  it('lists every icon the menus name once, for rendering', () => {
    const ids = menuIconIds(menus);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toContain('fileOpen');
    expect(ids).toContain('alignLeft');
    expect(ids).toContain('importVectors');
    // Cut, Copy and Paste stay system items and still name their icons.
    expect(ids).toContain('editCut');
  });
});
