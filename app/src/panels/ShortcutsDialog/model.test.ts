import { describe, expect, it } from 'vitest';

import { SHORTCUTS } from '../../lib/shortcuts';
import { commandLabel, keyLabel, sectionOf, shortcutSections } from './model';

describe('shortcutSections', () => {
  it('lists every bound command exactly once', () => {
    const rows = shortcutSections(false)
      .filter((s) => s.id !== 'nudge')
      .flatMap((s) => s.rows.map((r) => r.id));
    const commands = [...new Set(SHORTCUTS.map((s) => s.command))];
    expect([...rows].sort()).toEqual([...commands].sort());
  });

  it('gathers all the keys of a command', () => {
    const edit = shortcutSections(false).find((s) => s.id === 'edit');
    const redo = edit?.rows.find((r) => r.id === 'edit.redo');
    expect(redo?.keys).toEqual(['Ctrl+Y', 'Ctrl+Shift+Z']);
    const del = edit?.rows.find((r) => r.id === 'edit.delete');
    expect(del?.keys).toEqual(['Del', 'Backspace']);
  });

  it('gives every bound command words of its own', () => {
    for (const shortcut of SHORTCUTS) {
      expect(commandLabel(shortcut.command)).not.toBe(shortcut.command);
    }
  });

  it('ends with the arrow key nudges', () => {
    const sections = shortcutSections(true);
    const nudge = sections[sections.length - 1];
    expect(nudge?.id).toBe('nudge');
    expect(nudge?.rows.map((r) => r.id)).toEqual([
      'nudge.one',
      'nudge.ten',
      'nudge.tenth',
      'nudge.fixed',
    ]);
  });
});

describe('keyLabel', () => {
  it('writes keys for each platform', () => {
    expect(keyLabel({ key: 'z', ctrl: true, shift: true, command: 'edit.redo' }, false)).toBe(
      'Ctrl+Shift+Z',
    );
    expect(keyLabel({ key: 'z', ctrl: true, shift: true, command: 'edit.redo' }, true)).toBe('⇧⌘Z');
    expect(keyLabel({ key: 'PageUp', command: 'view.tileVertical' }, false)).toBe('Page Up');
    expect(keyLabel({ key: 'F2', command: 'view.mode2d' }, true)).toBe('F2');
  });
});

describe('sectionOf', () => {
  it('sorts commands by their menu', () => {
    expect(sectionOf('edit.undo')).toBe('edit');
    expect(sectionOf('view.mode3d')).toBe('view');
    expect(sectionOf('form.move')).toBe('panels');
    expect(sectionOf('tabs.drawing')).toBe('panels');
  });
});
