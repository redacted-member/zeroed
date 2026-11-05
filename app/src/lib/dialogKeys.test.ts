import type { KeyboardEvent } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { dialogKeyDown, modalDialogOpen } from './dialogKeys';

function keyEvent(key: string, defaultPrevented = false) {
  return {
    key,
    defaultPrevented,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  } as unknown as KeyboardEvent<HTMLElement> & {
    preventDefault: ReturnType<typeof vi.fn>;
    stopPropagation: ReturnType<typeof vi.fn>;
  };
}

describe('dialog keys', () => {
  it('closes on Escape and keeps every key from the drawing shortcuts', () => {
    const close = vi.fn();
    const handler = dialogKeyDown(close);
    const escape = keyEvent('Escape');
    handler(escape);
    expect(close).toHaveBeenCalledTimes(1);
    expect(escape.preventDefault).toHaveBeenCalled();
    expect(escape.stopPropagation).toHaveBeenCalled();
    const letter = keyEvent('a');
    handler(letter);
    expect(close).toHaveBeenCalledTimes(1);
    expect(letter.stopPropagation).toHaveBeenCalled();
  });

  it('leaves an Escape a control inside the dialog already used', () => {
    const close = vi.fn();
    dialogKeyDown(close)(keyEvent('Escape', true));
    expect(close).not.toHaveBeenCalled();
  });
});

describe('modal dialog open', () => {
  it('asks the document for an open dialog element', () => {
    let open = false;
    const querySelector = vi.fn((selector: string) =>
      selector.startsWith('dialog[open]') && open ? {} : null,
    );
    vi.stubGlobal('document', { querySelector });
    expect(modalDialogOpen()).toBe(false);
    open = true;
    expect(modalDialogOpen()).toBe(true);
    vi.unstubAllGlobals();
  });

  it('does not count a modeless dialog such as Component Properties', () => {
    // A stand in for the DOM with one open dialog marked modeless.
    const dialogs = [{ modeless: true }];
    const querySelector = vi.fn((selector: string) => {
      if (!selector.startsWith('dialog[open]')) {
        return null;
      }
      const skipModeless = selector.includes(':not([data-modeless])');
      return dialogs.find((d) => !(skipModeless && d.modeless)) ?? null;
    });
    vi.stubGlobal('document', { querySelector });
    expect(modalDialogOpen()).toBe(false);
    dialogs.push({ modeless: false });
    expect(modalDialogOpen()).toBe(true);
    vi.unstubAllGlobals();
  });
});
