import { beforeAll, describe, expect, it } from 'vitest';
import { loadExtraIcons, toolIcon } from '../../shell/ToolIcons/ToolIcons';

import type { ToolpathSummary } from '../../bridge/cam';
import { entriesWithoutIcon, menuItems, type PopupEntry } from '../../views/PopupMenu/model';
import { buildGroupMenu, buildListMenu, buildToolpathMenu } from './menu';

const toolpath: ToolpathSummary = {
  id: 3,
  name: 'Profile 1',
  kind: 'profile',
  visible: false,
  stale: false,
  toolName: 'End Mill (6 mm)',
  toolNumber: 1,
  estimatedSeconds: 60,
  moves: 10,
  sheet: 1,
};

const sheets = [
  { id: 1, name: 'Sheet 1' },
  { id: 2, name: 'Sheet 2' },
];

// The menu and form icons load lazily in the app.
beforeAll(loadExtraIcons);

describe('toolpath menus', () => {
  it('offers the commands in order', () => {
    const items = menuItems(buildToolpathMenu({ toolpath, canEdit: true, activeSheet: 1, sheets }));
    expect(items.map((i) => i.id)).toEqual([
      'visible',
      'showOnly',
      'showAllBut',
      'showSameTool',
      'showAll',
      'hideAll',
      'activateSheet',
      'edit',
      'rename',
      'duplicate',
      'recalculate',
      'recalculateVisible',
      'recalculateAll',
      'createGroup',
      'groupVisible',
      'ungroup',
      'sheet-2',
      'save',
      'delete',
      'deleteInvisible',
      'deleteVisible',
      'deleteAll',
    ]);
    expect(items[0]?.label).toBe('Show This');
    expect(items.find((i) => i.id === 'activateSheet')?.disabled).toBe(true);
    expect(items.find((i) => i.id === 'ungroup')?.disabled).toBe(true);
    expect(items.find((i) => i.id === 'sheet-2')?.action).toEqual({ moveToSheet: 2 });
  });

  it('disables Edit when no form edits this kind and follows the sheet', () => {
    const items = menuItems(
      buildToolpathMenu({
        toolpath: { ...toolpath, visible: true, sheet: 2, group: 4 },
        canEdit: false,
        activeSheet: 1,
        sheets,
      }),
    );
    expect(items.find((i) => i.id === 'edit')?.disabled).toBe(true);
    expect(items[0]?.action).toBe('hideThis');
    expect(items.find((i) => i.id === 'activateSheet')?.disabled).toBe(false);
    expect(items.find((i) => i.id === 'ungroup')?.disabled).toBe(false);
  });

  it('has group and list menus', () => {
    const group = { id: 1, name: 'G', sheet: 1, expanded: true };
    const items = menuItems(buildGroupMenu(group, 0));
    expect(items.find((i) => i.id === 'expand')?.label).toBe('Collapse');
    expect(items.find((i) => i.id === 'showGroup')?.disabled).toBe(true);
    expect(menuItems(buildListMenu(false)).map((i) => [i.id, i.disabled ?? false])).toEqual([
      ['createGroup', false],
      ['groupVisible', true],
    ]);
  });

  it('shows the Toolpaths tab icons on the items that run the same', () => {
    const items = menuItems(buildToolpathMenu({ toolpath, canEdit: true, activeSheet: 1, sheets }));
    const icon = (id: string) => items.find((i) => i.id === id)?.icon;
    expect([icon('edit'), icon('duplicate'), icon('recalculateAll'), icon('delete')]).toEqual([
      'editToolpath',
      'duplicateToolpath',
      'recalculateAll',
      'deleteToolpath',
    ]);
    const group = { id: 1, name: 'G', sheet: 0, expanded: true };
    const all = [
      ...items,
      ...menuItems(buildGroupMenu(group, 1)),
      ...menuItems(buildListMenu(true)),
    ];
    expect(all.filter((i) => i.icon !== undefined && toolIcon(i.icon) === null)).toEqual([]);
  });

  it('draws an icon on every item of every list menu', () => {
    const grouped = { ...toolpath, visible: true, group: 1 };
    const menus: PopupEntry<unknown>[][] = [
      buildToolpathMenu({ toolpath: grouped, canEdit: true, activeSheet: 2, sheets }),
      buildGroupMenu({ id: 1, name: 'G', sheet: 0, expanded: true }, 1),
      buildGroupMenu({ id: 1, name: 'G', sheet: 0, expanded: false }, 1),
      buildListMenu(true),
    ];
    expect(menus.flatMap((menu) => entriesWithoutIcon(menu))).toEqual([]);
    expect(
      menus.flatMap((menu) => menuItems(menu)).filter((i) => toolIcon(i.icon ?? '') === null),
    ).toEqual([]);
  });
});
