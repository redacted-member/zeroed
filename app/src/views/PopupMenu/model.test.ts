import { describe, expect, it } from 'vitest';

import {
  isSubmenu,
  menuItems,
  menuKey,
  submenuDisabled,
  withIcons,
  type PopupEntry,
} from './model';

type A = string;

const entries: PopupEntry<A>[] = [
  { id: 'one', label: 'One', action: 'one' },
  { separator: true },
  {
    id: 'more',
    label: 'More',
    entries: [
      { id: 'two', label: 'Two', action: 'two' },
      { id: 'three', label: 'Three', disabled: true, action: 'three' },
    ],
  },
];

describe('popup menu model', () => {
  it('lists the items of submenus too, separators dropped', () => {
    expect(menuItems(entries).map((i) => i.id)).toEqual(['one', 'two', 'three']);
    expect(entries.filter(isSubmenu).map((e) => e.id)).toEqual(['more']);
  });

  it('greys a submenu only when nothing in it can be picked', () => {
    const more = entries[2];
    expect(more !== undefined && isSubmenu(more) && submenuDisabled(more)).toBe(false);
    const none = {
      id: 'none',
      label: 'None',
      entries: [{ id: 'x', label: 'X', disabled: true, action: 'x' }],
    };
    expect(submenuDisabled(none)).toBe(true);
  });

  it('moves the focus with Up, Down, Home and End, wrapping round', () => {
    expect(menuKey('ArrowDown', 0, 3, false, false)).toEqual({ kind: 'focus', index: 1 });
    expect(menuKey('ArrowDown', 2, 3, false, false)).toEqual({ kind: 'focus', index: 0 });
    expect(menuKey('ArrowUp', 0, 3, false, false)).toEqual({ kind: 'focus', index: 2 });
    expect(menuKey('ArrowUp', -1, 3, false, false)).toEqual({ kind: 'focus', index: 2 });
    expect(menuKey('Home', 2, 3, false, false)).toEqual({ kind: 'focus', index: 0 });
    expect(menuKey('End', 0, 3, false, false)).toEqual({ kind: 'focus', index: 2 });
  });

  it('opens a submenu with Right, Enter or Space, and only on a submenu item', () => {
    for (const key of ['ArrowRight', 'Enter', ' ']) {
      expect(menuKey(key, 1, 3, true, false)).toEqual({ kind: 'open' });
    }
    expect(menuKey('ArrowRight', 0, 3, false, false)).toEqual({ kind: 'none' });
    // Enter on a plain item is the button's own click.
    expect(menuKey('Enter', 0, 3, false, false)).toEqual({ kind: 'none' });
  });

  it('closes a submenu with Left or Escape; Left does nothing in the top menu', () => {
    expect(menuKey('ArrowLeft', 0, 2, false, true)).toEqual({ kind: 'close' });
    expect(menuKey('Escape', 0, 2, false, true)).toEqual({ kind: 'close' });
    expect(menuKey('ArrowLeft', 0, 2, false, false)).toEqual({ kind: 'none' });
    expect(menuKey('Escape', 0, 2, false, false)).toEqual({ kind: 'close' });
  });

  it('gives items and submenus their icons by id, keeping an icon an entry names', () => {
    const entries: PopupEntry<string>[] = [
      { id: 'cut', label: 'Cut', action: 'cut' },
      { separator: true },
      { id: 'own', label: 'Own', icon: 'editCopy', action: 'own' },
      { id: 'more', label: 'More', entries: [{ id: 'cut', label: 'Cut', action: 'cut2' }] },
    ];
    const icons = { cut: 'editCut', own: 'editPaste', more: 'folder' };
    const out = withIcons(entries, icons);
    expect(menuItems(out).map((i) => i.icon)).toEqual(['editCut', 'editCopy', 'editCut']);
    const more = out[3];
    expect(more !== undefined && isSubmenu(more) ? more.icon : null).toBe('folder');
  });
});
