import { describe, expect, it } from 'vitest';

import type { Sheet, SheetMaterial } from '../../bridge/document';
import { loadExtraIcons, toolIcon } from '../../shell/ToolIcons/ToolIcons';
import { entriesWithoutIcon, menuItems } from '../../views/PopupMenu/model';
import {
  addSources,
  buildSheetMenu,
  canDelete,
  deleteWhy,
  deletePrompt,
  livePicks,
  nextSheetName,
  pickSheets,
  sheetSize,
  stepForKey,
  stepSheet,
} from './model';

const sheet = (
  id: number,
  name = `Sheet ${id}`,
  active = false,
  extra: Partial<Sheet> = {},
): Sheet => ({
  id,
  name,
  active,
  ...extra,
});

const material: SheetMaterial = {
  width: 609.6,
  height: 304.8,
  thickness: 12.7,
  zZero: 'top',
  xyOrigin: 'bottomLeft',
  offsetX: 0,
  offsetY: 0,
};

const plain = { toggle: false, range: false };

describe('sheets tab model', () => {
  it('proposes the next unused sheet name', () => {
    expect(nextSheetName([sheet(1)])).toBe('Sheet 2');
    expect(nextSheetName([sheet(1), sheet(4, 'Sheet 3')])).toBe('Sheet 4');
    expect(nextSheetName([sheet(1, 'sheet 2'), sheet(2, 'Offcut')])).toBe('Sheet 3');
    expect(nextSheetName([sheet(1)], ['Sheet 2'])).toBe('Sheet 3');
  });

  it('picks one sheet, toggles with Cmd and takes ranges with Shift', () => {
    const sheets = [sheet(1), sheet(2), sheet(3), sheet(4)];
    expect(pickSheets(sheets, [1, 2], 1, 3, plain)).toEqual([3]);
    expect(pickSheets(sheets, [1], 1, 3, { toggle: true, range: false })).toEqual([1, 3]);
    expect(pickSheets(sheets, [1, 3], 3, 3, { toggle: true, range: false })).toEqual([1]);
    expect(pickSheets(sheets, [4], 4, 2, { toggle: false, range: true })).toEqual([2, 3, 4]);
    expect(pickSheets(sheets, [], null, 2, { toggle: false, range: true })).toEqual([2]);
    expect(livePicks(sheets, [9, 3, 1])).toEqual([1, 3]);
  });

  it('adds sheets like the picked ones, or like the active one', () => {
    const sheets = [sheet(1, 'A', true), sheet(2, 'B'), sheet(3, 'C')];
    expect(addSources(sheets, []).map((s) => s.name)).toEqual(['A']);
    expect(addSources(sheets, [3, 2]).map((s) => s.name)).toEqual(['B', 'C']);
  });

  it('never deletes the last sheet', () => {
    const sheets = [sheet(1, 'A', true), sheet(2)];
    expect(canDelete(sheets, [])).toBe(false);
    expect(canDelete(sheets, [2])).toBe(true);
    expect(canDelete(sheets, [1, 2])).toBe(false);
  });

  it('says why the picked sheets cannot be deleted', () => {
    const sheets = [sheet(1, 'A', true), sheet(2)];
    expect(deleteWhy(sheets, [])).toBe('Pick a sheet in the list first.');
    expect(deleteWhy(sheets, [2])).toBeUndefined();
    expect(deleteWhy(sheets, [1, 2])).toBe('A job needs at least one sheet.');
  });

  it('asks before deleting sheets that hold vectors or components', () => {
    expect(deletePrompt([sheet(2, 'Offcut')])).toBeNull();
    expect(deletePrompt([sheet(2, 'Offcut', false, { count: 3 })])).toContain('Offcut');
    expect(deletePrompt([sheet(2, 'Offcut', false, { components: 1 })])).toContain('1 object?');
    expect(
      deletePrompt([sheet(2, 'A', false, { count: 2 }), sheet(3, 'B', false, { components: 1 })]),
    ).toContain('2 sheets and the 3 objects');
  });

  it('shows the sheet size in the job unit', () => {
    expect(sheetSize(sheet(1), 'mm')).toBe('');
    expect(sheetSize(sheet(1, 'A', true, { material }), 'mm')).toBe('609.6 x 304.8 x 12.7 mm');
    expect(sheetSize(sheet(1, 'A', true, { material }), 'inch')).toBe('24 x 12 x 0.5 inch');
  });

  it('steps through the list without wrapping', () => {
    const sheets = [sheet(1), sheet(5), sheet(9)];
    expect(stepSheet(sheets, 1, 'next')).toBe(5);
    expect(stepSheet(sheets, 9, 'next')).toBe(9);
    expect(stepSheet(sheets, 1, 'previous')).toBe(1);
    expect(stepSheet(sheets, 5, 'first')).toBe(1);
    expect(stepSheet(sheets, 5, 'last')).toBe(9);
    expect(stepSheet([], 1, 'next')).toBeNull();
    expect(stepForKey('ArrowDown')).toBe('next');
    expect(stepForKey('ArrowUp')).toBe('previous');
    expect(stepForKey('a')).toBeNull();
  });

  it('builds the right click menu', () => {
    const items = menuItems(buildSheetMenu({ sheet: sheet(1, 'Sheet 1', true), sheetCount: 1 }));
    expect(items.map((i) => i.action)).toEqual([
      'edit',
      'rename',
      'makeActive',
      'duplicate',
      'moveHere',
      'delete',
      'newSheet',
    ]);
    expect(items.find((i) => i.action === 'moveHere')?.disabled).toBe(true);
    expect(items.find((i) => i.action === 'makeActive')?.disabled).toBe(true);
    expect(items.find((i) => i.action === 'delete')?.disabled).toBe(true);
    const other = menuItems(buildSheetMenu({ sheet: sheet(2), sheetCount: 2, selectedCount: 3 }));
    expect(other.every((i) => i.disabled !== true)).toBe(true);
    const nothingSelected = menuItems(buildSheetMenu({ sheet: sheet(2), sheetCount: 2 }));
    expect(nothingSelected.find((i) => i.action === 'moveHere')?.disabled).toBe(true);
  });

  it('draws an icon on every item of the right click menu', async () => {
    await loadExtraIcons();
    const menu = buildSheetMenu({ sheet: sheet(2), sheetCount: 2, selectedCount: 1 });
    expect(entriesWithoutIcon(menu)).toEqual([]);
    expect(menuItems(menu).filter((i) => toolIcon(i.icon ?? '') === null)).toEqual([]);
  });
});
