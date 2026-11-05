import { describe, expect, it } from 'vitest';

import { SHORTCUTS, TRANSFORM_FORM_KEYS, commandForKey, nudgeForKey } from './shortcuts';

function keyEvent(init: KeyboardEventInit): KeyboardEvent {
  // The unit test runner has no DOM; the functions only read these fields.
  return {
    key: init.key ?? '',
    code: init.code ?? '',
    ctrlKey: init.ctrlKey ?? false,
    metaKey: init.metaKey ?? false,
    shiftKey: init.shiftKey ?? false,
    altKey: init.altKey ?? false,
  } as KeyboardEvent;
}

describe('shortcut table', () => {
  it('binds every key combination at most once', () => {
    const seen = new Set<string>();
    for (const shortcut of SHORTCUTS) {
      const signature = [
        shortcut.key.toLowerCase(),
        shortcut.ctrl ?? false,
        shortcut.shift ?? false,
        shortcut.alt ?? false,
      ].join('|');
      expect(seen.has(signature), `duplicate binding ${signature}`).toBe(false);
      seen.add(signature);
    }
  });

  it('maps the defaults', () => {
    expect(commandForKey(keyEvent({ key: 'z', ctrlKey: true }))).toBe('edit.undo');
    expect(commandForKey(keyEvent({ key: 'z', metaKey: true }))).toBe('edit.undo');
    expect(commandForKey(keyEvent({ key: 'Z', ctrlKey: true, shiftKey: true }))).toBe('edit.redo');
    expect(commandForKey(keyEvent({ key: 'n' }))).toBe('tool.nodeToggle');
    expect(commandForKey(keyEvent({ key: 'F6' }))).toBe('view.zoomToJob');
    expect(commandForKey(keyEvent({ key: 'f', ctrlKey: true }))).toBe('view.zoomToSelection');
    expect(commandForKey(keyEvent({ key: '9' }))).toBe('edit.rotateCcw45');
    expect(commandForKey(keyEvent({ key: 'h', ctrlKey: true }))).toBe('edit.mirrorHorizontalCopy');
    expect(commandForKey(keyEvent({ key: 'q' }))).toBeNull();
    expect(commandForKey(keyEvent({ key: 's', metaKey: true }))).toBe('file.save');
    expect(commandForKey(keyEvent({ key: 'i', ctrlKey: true }))).toBe('file.importVectorsBitmaps');
    expect(commandForKey(keyEvent({ key: 'd', ctrlKey: true }))).toBe('tabs.drawingTab');
    expect(commandForKey(keyEvent({ key: 'l', ctrlKey: true }))).toBe('tabs.layers');
    expect(commandForKey(keyEvent({ key: 'm', ctrlKey: true }))).toBe('form.measure');
    expect(commandForKey(keyEvent({ key: 'j' }))).toBe('form.join');
    expect(commandForKey(keyEvent({ key: 'z' }))).toBe('tool.zoomBox');
    expect(commandForKey(keyEvent({ key: 'U', shiftKey: true }))).toBe('edit.ungroupDeep');
    expect(commandForKey(keyEvent({ key: 'u', ctrlKey: true }))).toBe('edit.ungroupToGroupLayer');
    expect(commandForKey(keyEvent({ key: 'U', ctrlKey: true, shiftKey: true }))).toBe(
      'edit.ungroupDeepToGroupLayer',
    );
    expect(commandForKey(keyEvent({ key: 'H', ctrlKey: true, shiftKey: true }))).toBe(
      'edit.mirrorMaterialHorizontalCopy',
    );
    expect(commandForKey(keyEvent({ key: 'V', ctrlKey: true, shiftKey: true }))).toBe(
      'edit.mirrorMaterialVerticalCopy',
    );
  });

  it('scales arrow nudges by the modifiers', () => {
    expect(nudgeForKey(keyEvent({ key: 'ArrowLeft' }))).toEqual({ dx: -1, dy: 0 });
    expect(nudgeForKey(keyEvent({ key: 'ArrowUp', shiftKey: true }))).toEqual({ dx: 0, dy: 10 });
    expect(nudgeForKey(keyEvent({ key: 'ArrowDown', ctrlKey: true }))).toEqual({ dx: 0, dy: -0.1 });
    expect(nudgeForKey(keyEvent({ key: 'a' }))).toBeNull();
    expect(
      nudgeForKey(keyEvent({ key: 'ArrowRight', ctrlKey: true, shiftKey: true }), 2.5),
    ).toEqual({ dx: 2.5, dy: 0 });
  });
});

describe('side shortcuts', () => {
  it('shows the top side on 1 and the bottom side on 2', () => {
    expect(commandForKey(keyEvent({ key: '1' }))).toBe('view.sideTop');
    expect(commandForKey(keyEvent({ key: '2' }))).toBe('view.sideBottom');
  });
});

describe('transform and tiling keys', () => {
  it('opens Set Size, Move and Rotate on T, M and R', () => {
    const formFor = (key: string) => {
      const command = commandForKey(keyEvent({ key }));
      return command !== null && command in TRANSFORM_FORM_KEYS
        ? TRANSFORM_FORM_KEYS[command as keyof typeof TRANSFORM_FORM_KEYS]
        : null;
    };
    expect(formFor('t')).toBe('setSize');
    expect(formFor('m')).toBe('move');
    expect(formFor('r')).toBe('rotate');
    expect(formFor('F10')).toBe('align');
  });

  it('tiles the views on Page Up and Page Down', () => {
    expect(commandForKey(keyEvent({ key: 'PageUp' }))).toBe('view.tileVertical');
    expect(commandForKey(keyEvent({ key: 'PageDown' }))).toBe('view.tileHorizontal');
  });

  it('shows the model volume on Ctrl+Shift+Alt+V, Option letters included', () => {
    const volume = { ctrlKey: true, shiftKey: true, altKey: true };
    expect(commandForKey(keyEvent({ ...volume, key: 'V', code: 'KeyV' }))).toBe('view.modelVolume');
    // A Mac keyboard reports Option+Shift+V as another character.
    expect(commandForKey(keyEvent({ ...volume, key: '◊', code: 'KeyV' }))).toBe('view.modelVolume');
    // Ctrl+Shift+V stays the mirrored copy about the material centre, and
    // Ctrl+D the Drawing tab.
    expect(commandForKey(keyEvent({ ctrlKey: true, shiftKey: true, key: 'V' }))).toBe(
      'edit.mirrorMaterialVerticalCopy',
    );
    expect(commandForKey(keyEvent({ ctrlKey: true, key: 'd' }))).toBe('tabs.drawingTab');
  });
});
