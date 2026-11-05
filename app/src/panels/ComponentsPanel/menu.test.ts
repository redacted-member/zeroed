import { beforeAll, describe, expect, it } from 'vitest';
import { loadExtraIcons, toolIcon } from '../../shell/ToolIcons/ToolIcons';

import type { ComponentGroup } from '../../bridge/componentTree';
import type { ComponentSummary, Level } from '../../bridge/model';
import {
  entriesWithoutIcon,
  isSubmenu,
  menuItems,
  submenuDisabled,
  type PopupEntry,
} from '../../views/PopupMenu/model';
import { EDIT_OPS } from '../../forms/ComponentEditForm/model';
import { buildComponentMenu, buildGroupMenu, buildLevelMenu } from './menu';

const component: ComponentSummary = {
  id: 7,
  name: 'Dome',
  visible: true,
  combine: 'subtract',
  heightScale: 1,
  level: 1,
  maxHeight: 5,
};
const levels: Level[] = [
  { id: 1, name: 'Level 1', visible: true, combine: 'add' },
  { id: 2, name: 'Level 2', visible: true, combine: 'add', clip: [4] },
];
const combines = [
  { value: 'add', label: 'Add' },
  { value: 'subtract', label: 'Subtract' },
  { value: 'multiply', label: 'Multiply' },
] as const;
const edits = [{ op: 'tilt', label: 'Tilt' }] as const;

// The menu and form icons load lazily in the app.
beforeAll(loadExtraIcons);

describe('component menu', () => {
  it('offers Reimport only on an imported component', () => {
    const menu = (c: ComponentSummary) =>
      menuItems(buildComponentMenu({ component: c, picked: 1, levels, combines, edits }));
    expect(menu(component).some((i) => i.id === 'reimport')).toBe(false);
    expect(menu(component).some((i) => i.id === 'moveToOtherSide')).toBe(false);
    const twoSided = menuItems(
      buildComponentMenu({ component, picked: 1, levels, combines, edits, doubleSided: true }),
    );
    expect(twoSided.find((i) => i.id === 'copyToOtherSide')?.action).toEqual({
      kind: 'otherSide',
      copy: true,
    });
    expect(twoSided.find((i) => i.id === 'moveToOtherSide')?.action).toEqual({
      kind: 'otherSide',
      copy: false,
    });
    const item = menu({ ...component, imported: true }).find((i) => i.id === 'reimport');
    expect(item?.action).toEqual({ kind: 'reimport' });
  });

  it('marks the current combine mode and level', () => {
    const items = menuItems(buildComponentMenu({ component, picked: 1, levels, combines, edits }));
    expect(items[0]?.id).toBe('properties');
    expect(items.find((i) => i.id === 'combine-subtract')?.checked).toBe(true);
    expect(items.find((i) => i.id === 'combine-multiply')?.action).toEqual({
      kind: 'combine',
      combine: 'multiply',
    });
    expect(items.find((i) => i.id === 'level-1')?.checked).toBe(true);
    expect(items.find((i) => i.id === 'level-2')?.action).toEqual({ kind: 'level', level: 2 });
    expect(items.find((i) => i.id === 'edit-tilt')?.action).toEqual({ kind: 'edit', op: 'tilt' });
    expect(items.find((i) => i.id === 'bake')?.disabled).toBeUndefined();
    expect(items.some((i) => i.id === 'removeFromGroup')).toBe(false);
  });

  it('leaves out levels with only one, and single item actions with several picked', () => {
    const items = menuItems(
      buildComponentMenu({
        component: { ...component, group: 3 },
        picked: 2,
        levels: levels.slice(0, 1),
        combines,
        edits,
      }),
    );
    expect(items.some((i) => i.id.startsWith('level-'))).toBe(false);
    expect(items.find((i) => i.id === 'bake')?.disabled).toBeUndefined();
    expect(items.find((i) => i.id === 'rename')?.disabled).toBe(true);
    expect(items.find((i) => i.id === 'edit-tilt')?.disabled).toBe(true);
    expect(items.find((i) => i.id === 'createBrush')?.disabled).toBe(true);
    expect(items.find((i) => i.id === 'group')?.disabled).toBeUndefined();
    expect(items.map((i) => i.id)).toEqual(
      expect.arrayContaining(['visible', 'removeFromGroup', 'fromSelected', 'delete']),
    );
  });
});

describe('group and level menus', () => {
  it('a group can be ungrouped and recombined', () => {
    const group: ComponentGroup = {
      id: 3,
      name: 'Leaves',
      visible: false,
      combine: 'add',
      expanded: true,
      color: null,
    };
    const items = menuItems(buildGroupMenu(group, combines));
    expect(items.find((i) => i.id === 'visible')?.label).toBe('Show');
    expect(items.find((i) => i.id === 'combine-add')?.checked).toBe(true);
    expect(items.map((i) => i.id)).toContain('ungroup');
  });

  it('a level holds Apply, Update and Remove in a Clipping submenu', () => {
    const entries = buildLevelMenu({
      level: levels[1] as Level,
      index: 1,
      count: 2,
      active: false,
      vectorsSelected: false,
      combines,
    });
    const clipping = entries.find((e) => isSubmenu(e) && e.id === 'clipping');
    expect(clipping !== undefined && isSubmenu(clipping)).toBe(true);
    if (clipping === undefined || !isSubmenu(clipping)) {
      return;
    }
    expect(clipping.label).toBe('Clipping');
    expect(menuItems(clipping.entries).map((i) => [i.id, i.label])).toEqual([
      ['clip', 'Apply'],
      ['updateClip', 'Update'],
      ['clearClip', 'Remove'],
    ]);
    // Not listed flat as well.
    expect(entries.some((e) => !isSubmenu(e) && 'id' in e && e.id === 'clip')).toBe(false);
    expect(submenuDisabled(clipping)).toBe(false);
  });

  it('a level clips to selected vectors and moves within the list', () => {
    const first = menuItems(
      buildLevelMenu({
        level: levels[0] as Level,
        index: 0,
        count: 2,
        active: true,
        vectorsSelected: false,
        combines,
      }),
    );
    expect(first.find((i) => i.id === 'clip')?.disabled).toBe(true);
    expect(first.find((i) => i.id === 'clearClip')?.disabled).toBe(true);
    // The base level sits at the bottom of the bottom up list.
    expect(first.find((i) => i.id === 'down')?.disabled).toBe(true);
    expect(first.find((i) => i.id === 'up')?.disabled).toBe(false);
    expect(first.find((i) => i.id === 'makeActive')?.checked).toBe(true);
    const second = menuItems(
      buildLevelMenu({
        level: levels[1] as Level,
        index: 1,
        count: 2,
        active: false,
        vectorsSelected: true,
        combines,
      }),
    );
    expect(second.find((i) => i.id === 'clip')?.disabled).toBe(false);
    expect(second.find((i) => i.id === 'clearClip')?.disabled).toBe(false);
    expect(second.find((i) => i.id === 'updateClip')?.disabled).toBe(false);
    expect(first.find((i) => i.id === 'updateClip')?.disabled).toBe(true);
    expect(second.find((i) => i.id === 'up')?.disabled).toBe(true);
    expect(first.find((i) => i.id === 'mirror-none')?.checked).toBe(true);
    const mirrored = menuItems(
      buildLevelMenu({
        level: { ...(levels[0] as Level), mirror: 'topLeft' },
        index: 0,
        count: 2,
        active: true,
        vectorsSelected: false,
        combines,
      }),
    );
    expect(mirrored.filter((i) => i.id.startsWith('mirror-'))).toHaveLength(9);
    expect(mirrored.find((i) => i.id === 'mirror-topLeft')?.checked).toBe(true);
    expect(mirrored.find((i) => i.id === 'mirror-none')?.action).toEqual({
      kind: 'mirror',
      mirror: null,
    });
  });

  it('shows the Modeling Tools icons, all of them drawn', () => {
    const items = [
      ...menuItems(buildComponentMenu({ component, picked: 1, levels, combines, edits })),
      ...menuItems(
        buildLevelMenu({
          level: { id: 2, name: 'Level 2', visible: true, combine: 'add', clip: [4] },
          index: 1,
          count: 2,
          active: false,
          vectorsSelected: true,
          combines,
        }),
      ),
    ];
    expect(items.find((i) => i.id === 'bake')?.icon).toBe('bakeComponents');
    expect(items.filter((i) => i.icon !== undefined && toolIcon(i.icon) === null)).toEqual([]);
  });

  it('draws an icon on every item and submenu, combine and mirror marks included', () => {
    const all = [
      { value: 'add', label: 'Add' },
      { value: 'subtract', label: 'Subtract' },
      { value: 'mergeHigh', label: 'Merge High' },
      { value: 'mergeLow', label: 'Merge Low' },
      { value: 'multiply', label: 'Multiply' },
    ] as const;
    const everyEdit = EDIT_OPS.map((op) => ({ op, label: op }));
    const menus: PopupEntry<unknown>[][] = [
      buildComponentMenu({
        component: { ...component, group: 3, imported: true },
        picked: 1,
        levels,
        combines: all,
        edits: everyEdit,
        doubleSided: true,
      }),
      buildGroupMenu(
        { id: 3, name: 'Group', visible: true, combine: 'add', expanded: true, color: '#808080' },
        all,
      ),
      buildLevelMenu({
        level: { id: 2, name: 'Level 2', visible: true, combine: 'add', clip: [4] },
        index: 1,
        count: 3,
        active: true,
        vectorsSelected: true,
        combines: all,
      }),
    ];
    expect(menus.flatMap((menu) => entriesWithoutIcon(menu))).toEqual([]);
    expect(
      menus.flatMap((menu) => menuItems(menu)).filter((i) => toolIcon(i.icon ?? '') === null),
    ).toEqual([]);
  });
});
