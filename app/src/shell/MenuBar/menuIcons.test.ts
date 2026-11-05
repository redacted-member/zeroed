import { beforeAll, describe, expect, it } from 'vitest';

import { IDLE_STATE } from '../../lib/enableState';
import { defaultPrefs } from '../../lib/prefs';
import { TRANSFORM_FORM_KEYS, type AppCommand } from '../../lib/shortcuts';
import { TOOL_GROUPS } from '../LeftPanel/toolGroups';
import { loadExtraIcons, toolIcon } from '../ToolIcons/ToolIcons';
import { displayState } from './displayState';
import { FORMS } from './menuCommands';
import { menuIcon } from './menuIcons';
import { buildMenus, flatItems, type MenuContext, type MenuEntry } from './model';

const CTX: MenuContext = {
  recentFiles: ['/jobs/sign.carve'],
  gadgets: [],
  gadgetsLoaded: true,
  gadgetFailure: null,
  guidesVisible: true,
  toolpathsPinned: true,
  previewQuality: 'standard',
  showBlock: true,
  shadedBackground: false,
  display: displayState(defaultPrefs(), false),
  enable: IDLE_STATE,
  mac: false,
};

function allItems() {
  return buildMenus(CTX).flatMap((menu) => flatItems(menu.entries));
}

/** The ids of the items and submenus of a menu that draw no icon, info lines aside. */
function withoutIcon(entries: readonly MenuEntry[]): string[] {
  return entries.flatMap((e): string[] => {
    if (e.kind === 'separator') {
      return [];
    }
    if (e.kind === 'submenu') {
      return [...(e.icon === undefined ? [e.id] : []), ...withoutIcon(e.entries)];
    }
    return e.icon === undefined && e.info !== true ? [e.id] : [];
  });
}

function submenus(entries: readonly MenuEntry[]): MenuEntry[] {
  return entries.flatMap((e) => (e.kind === 'submenu' ? [e, ...submenus(e.entries)] : []));
}

// The menu and form icons load lazily in the app.
beforeAll(loadExtraIcons);

describe('menu icons', () => {
  it('shows an icon on every item that runs a tool or opens a tool form', () => {
    const toolCommands = new Set<string>(
      Object.values(TOOL_GROUPS)
        .flat()
        .flatMap((g) => g.tools)
        .flatMap((t) => (t.command === undefined ? [] : [t.command])),
    );
    const formCommands = new Set<string>([
      ...Object.keys(FORMS),
      ...Object.keys(TRANSFORM_FORM_KEYS),
    ]);
    const missing = allItems()
      .filter((item) => item.action.type === 'command')
      .filter((item) => toolCommands.has(item.id) || formCommands.has(item.id))
      .filter((item) => item.icon === undefined)
      .map((item) => item.id);
    expect(missing).toEqual([]);
  });

  it('shows an icon on every item and submenu, check items too; only info lines stay blank', () => {
    const missing = (menus: readonly { entries: readonly MenuEntry[] }[]) =>
      menus.flatMap((menu) => withoutIcon(menu.entries));
    expect(missing(buildMenus(CTX))).toEqual([]);
    // Every state the lists change with: recent files, gadgets loading, failed or none.
    expect(missing(buildMenus({ ...CTX, recentFiles: [] }))).toEqual([]);
    expect(missing(buildMenus({ ...CTX, gadgetsLoaded: false }))).toEqual([]);
    expect(missing(buildMenus({ ...CTX, gadgetFailure: 'broken' }))).toEqual([]);
  });

  it('draws every icon it names', () => {
    const named = [
      ...allItems().flatMap((i) => (i.icon === undefined ? [] : [i.icon])),
      ...buildMenus(CTX)
        .flatMap((m) => submenus(m.entries))
        .flatMap((s) => (s.kind === 'submenu' && s.icon !== undefined ? [s.icon] : [])),
    ];
    expect(named.filter((id) => toolIcon(id) === null)).toEqual([]);
  });

  it('shows the icon of a check or radio item beside its mark, and none on info lines', () => {
    const items = allItems();
    const marked = items.filter((i) => i.checked !== undefined);
    expect(marked.length).toBeGreaterThan(20);
    expect(marked.filter((i) => i.icon === undefined)).toEqual([]);
    expect(menuIcon('view.guidesVisible')).toBe('guidesVisible');
    expect(menuIcon('preview.maximum')).toBe('quality5');
    const info = items.filter((i) => i.info === true);
    expect(info.map((i) => i.id)).toEqual(['gadgets.empty']);
    expect(info.filter((i) => i.icon !== undefined || i.disabled !== true)).toEqual([]);
  });

  it('shows the gadget icon on every gadget and icons on the Machine and Help items', () => {
    const gadget = {
      name: 'gear',
      title: 'Gear Maker',
      description: '',
      origin: 'bundled',
    } as const;
    const items = buildMenus({ ...CTX, gadgets: [gadget] }).flatMap((menu) =>
      flatItems(menu.entries),
    );
    expect(items.find((item) => item.id === 'gadget.gear')?.icon).toBe('gadget');
    const blank = items
      .filter((item) => /^(machine|help|gadgets)\./.test(item.id) && item.icon === undefined)
      .map((item) => item.id);
    expect(blank).toEqual([]);
  });

  it('uses the Design tab tool icon for a command the tab runs too', () => {
    expect(menuIcon('align.lastLeft')).toBe('alignLeft');
    expect(menuIcon('model.bake')).toBe('bakeComponents');
    expect(menuIcon('edit.undo')).toBe('editUndo');
    expect(menuIcon('edit.cut' as AppCommand)).toBe('editCut');
    expect(menuIcon('view.cascade')).toBe('cascade');
    expect(menuIcon('file.close')).toBe('fileClose');
  });
});
