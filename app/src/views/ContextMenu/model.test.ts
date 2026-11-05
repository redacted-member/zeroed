import { beforeAll, describe, expect, it } from 'vitest';
import { loadExtraIcons, toolIcon } from '../../shell/ToolIcons/ToolIcons';

import { IS_MAC } from '../../lib/keyLabels';
import { shortcutLabel } from '../../shell/MenuBar/model';
import { entriesWithoutIcon } from '../PopupMenu/model';
import { buildMenu, controllerIcon, isSeparator, type MenuContext, type MenuItem } from './model';

const base: MenuContext = {
  kind: 'canvas',
  selectedCount: 0,
  clipboardCount: 0,
  canUndo: false,
  canRedo: false,
  undoLabel: null,
  redoLabel: null,
  hasGroup: false,
};

// The menu and form icons load lazily in the app.
beforeAll(loadExtraIcons);

describe('context menu model', () => {
  it('disables edits without a selection and enables them with one', () => {
    const items = (entries: ReturnType<typeof buildMenu>): MenuItem[] =>
      entries.filter((e): e is MenuItem => !isSeparator(e));
    const empty = items(buildMenu(base));
    const byId = (id: string) => empty.find((e) => e.id === id);
    expect(byId('cut')?.disabled).toBe(true);
    expect(byId('paste')?.disabled).toBe(true);
    expect(byId('undo')?.disabled).toBe(true);
    const withSelection = buildMenu({
      ...base,
      selectedCount: 2,
      clipboardCount: 1,
      canUndo: true,
      undoLabel: 'Move',
    });
    const item = (id: string) => items(withSelection).find((e) => e.id === id);
    expect(item('cut')?.disabled).toBe(false);
    expect(item('paste')?.disabled).toBe(false);
    expect(item('group')?.disabled).toBe(false);
    expect(item('nodeEdit')?.disabled).toBe(true);
    expect(item('undo')?.label).toContain('Move');
  });

  it('offers one node menu over a node and another over a span', () => {
    const keysOf = (hover: 'node' | 'span' | null) =>
      buildMenu({ ...base, kind: 'node', nodeHover: hover })
        .filter((e): e is MenuItem => !isSeparator(e))
        .map((e) => ('toolKey' in e.action ? e.action.toolKey : e.action.command));
    const node = keysOf('node');
    expect(node).toEqual(expect.arrayContaining(['d', 's', 'c', 'p', 'j', 'x', 'properties']));
    expect(node).not.toContain('i');
    expect(node).not.toContain('l');
    const span = keysOf('span');
    expect(span).toEqual(expect.arrayContaining(['i', 'insert-midpoint', 'd', 'l', 'a', 'b']));
    expect(span).not.toContain('s');
    for (const keys of [node, span, keysOf(null)]) {
      expect(keys).toEqual(expect.arrayContaining(['reverse', 'close', 'k', 'tool.select']));
    }
  });

  it('shows only the keys that run the same command', () => {
    const items = buildMenu({ ...base, selectedCount: 2, clipboardCount: 1 }).filter(
      (e): e is MenuItem => !isSeparator(e),
    );
    const wrong = items.flatMap((item) => {
      const action = item.action;
      if (item.shortcut === undefined || !('command' in action)) {
        return [];
      }
      const bound = shortcutLabel(action.command, IS_MAC);
      return bound === item.shortcut
        ? []
        : [`${item.id}: ${item.shortcut} runs ${bound ?? 'nothing'}`];
    });
    expect(wrong).toEqual([]);
  });

  it('keeps ids unique', () => {
    const ids = buildMenu({ ...base, selectedCount: 1 })
      .filter((e) => !isSeparator(e))
      .map((e) => (isSeparator(e) ? '' : e.id));
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe('part and picture entries', () => {
  const items = (entries: ReturnType<typeof buildMenu>): MenuItem[] =>
    entries.filter((e): e is MenuItem => !isSeparator(e));

  it('selects the vectors inside a closed boundary', () => {
    const off = items(buildMenu({ ...base, selectedCount: 1 })).find(
      (e) => e.id === 'selectInside',
    );
    expect(off?.disabled).toBe(true);
    const on = items(buildMenu({ ...base, selectedCount: 1, hasClosedSelection: true })).find(
      (e) => e.id === 'selectInside',
    );
    expect(on?.disabled).toBe(false);
    expect(on?.action).toEqual({ command: 'select.insideBoundary' });
  });

  it('shows the picture tools only for a selected bitmap', () => {
    expect(items(buildMenu(base)).some((e) => e.id === 'editPicture')).toBe(false);
    const withBitmap = items(buildMenu({ ...base, selectedCount: 1, hasBitmap: true }));
    expect(withBitmap.find((e) => e.id === 'editPicture')?.disabled).toBeUndefined();
    expect(withBitmap.find((e) => e.id === 'cropBitmap')?.disabled).toBe(true);
    const canCrop = items(
      buildMenu({ ...base, selectedCount: 2, hasBitmap: true, canCropBitmap: true }),
    );
    expect(canCrop.find((e) => e.id === 'cropBitmap')?.disabled).toBe(false);
  });

  it('offers Remove Text From The Curve for text on a curve', () => {
    expect(items(buildMenu(base)).some((e) => e.id === 'removeFromCurve')).toBe(false);
    const onCurve = items(buildMenu({ ...base, selectedCount: 1, hasCurveText: true }));
    expect(onCurve.find((e) => e.id === 'removeFromCurve')?.action).toEqual({
      command: 'text.removeFromCurve',
    });
  });

  it('runs the drawing tools the menu bar leaves out: Exclusive Or, text adjust, view image', () => {
    const one = items(buildMenu({ ...base, selectedCount: 1 }));
    expect(one.find((e) => e.id === 'xor')?.disabled).toBe(true);
    expect(one.some((e) => e.id === 'adjustText')).toBe(false);
    const two = items(buildMenu({ ...base, selectedCount: 2 }));
    expect(two.find((e) => e.id === 'xor')?.action).toEqual({ command: 'edit.xor' });
    expect(two.find((e) => e.id === 'xor')?.disabled).toBe(false);
    const text = items(buildMenu({ ...base, selectedCount: 1, hasText: true }));
    expect(text.find((e) => e.id === 'adjustText')?.action).toEqual({ command: 'text.adjust' });
    expect(items(buildMenu(base)).find((e) => e.id === 'saveViewImage')?.action).toEqual({
      command: 'file.saveViewImage',
    });
  });

  it('shows the menu icons of the commands, and node tool icons where they apply', () => {
    const canvas = buildMenu(base).filter((e): e is MenuItem => !isSeparator(e));
    const icon = (id: string) => canvas.find((e) => e.id === id)?.icon;
    expect([icon('undo'), icon('cut'), icon('copy'), icon('paste'), icon('delete')]).toEqual([
      'editUndo',
      'editCut',
      'editCopy',
      'editPaste',
      'editDelete',
    ]);
    expect(icon('group')).toBe('group');
    expect(icon('zoomJob')).toBe('zoomJob');
    const node = buildMenu({ ...base, kind: 'node', nodeHover: 'node' }).filter(
      (e): e is MenuItem => !isSeparator(e),
    );
    expect(node.find((e) => e.id === 'join')?.icon).toBe('join');
    const all = [...canvas, ...node];
    expect(all.filter((e) => e.icon !== undefined && toolIcon(e.icon) === null)).toEqual([]);
  });

  it('draws an icon on every item of the 2D view and node editing menus', () => {
    const full: MenuContext = {
      ...base,
      selectedCount: 2,
      clipboardCount: 1,
      canUndo: true,
      canRedo: true,
      hasGroup: true,
      hasClosedSelection: true,
      hasBitmap: true,
      canCropBitmap: true,
      hasCurveText: true,
      hasText: true,
    };
    const menus = [
      buildMenu(full),
      ...(['node', 'span', null] as const).map((nodeHover) =>
        buildMenu({ ...full, kind: 'node', nodeHover }),
      ),
    ];
    expect(menus.flatMap((menu) => entriesWithoutIcon(menu))).toEqual([]);
    const items = menus.flat().filter((e): e is MenuItem => !isSeparator(e));
    expect(items.filter((e) => toolIcon(e.icon ?? '') === null)).toEqual([]);
  });

  it('draws an icon on every item the sweep forms offer on right click', () => {
    const ids = [
      'reverse',
      'makeStart',
      'insertStart',
      'removeAll',
      'deleteSection',
      'smooth',
      'allNodes',
    ];
    expect(ids.filter((id) => toolIcon(controllerIcon(id) ?? '') === null)).toEqual([]);
  });
});
