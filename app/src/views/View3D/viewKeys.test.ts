import { afterEach, describe, expect, it, vi } from 'vitest';

import { endsTransform, shadowKeyStep, startsTwiddle, typingIn, type ViewKey } from './viewKeys';

/** A target that stands in for a DOM element. */
function element(fields: object): EventTarget {
  return fields as unknown as EventTarget;
}

function key(fields: Partial<ViewKey>): ViewKey {
  return {
    key: '',
    code: '',
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    target: null,
    ...fields,
  };
}

/** A stand in for the DOM whose modal dialog opens on request. */
function stubDialog(): { open: boolean } {
  const state = { open: false };
  vi.stubGlobal('document', {
    querySelector: (selector: string) =>
      selector.startsWith('dialog[open]') && state.open ? {} : null,
  });
  return state;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('3D view keys', () => {
  it('steps the shadow shading with Ctrl + NumPad plus or minus', () => {
    stubDialog();
    expect(shadowKeyStep(key({ code: 'NumpadAdd', ctrlKey: true }), true)).toBe(1);
    expect(shadowKeyStep(key({ code: 'NumpadSubtract', ctrlKey: true }), true)).toBe(-1);
    expect(shadowKeyStep(key({ code: 'NumpadAdd' }), true)).toBe(0);
    expect(shadowKeyStep(key({ code: 'NumpadAdd', ctrlKey: true }), false)).toBe(0);
    expect(shadowKeyStep(key({ code: 'KeyA', ctrlKey: true }), true)).toBe(0);
  });

  it('twiddles on W over the view, not while typing', () => {
    stubDialog();
    expect(startsTwiddle(key({ key: 'w' }), true)).toBe(true);
    expect(startsTwiddle(key({ key: 'W' }), true)).toBe(true);
    expect(startsTwiddle(key({ key: 'w' }), false)).toBe(false);
    expect(startsTwiddle(key({ key: 'w', target: element({ tagName: 'INPUT' }) }), true)).toBe(
      false,
    );
  });

  it('puts the transform handles away on Escape when no handle is dragged', () => {
    stubDialog();
    expect(endsTransform(key({ key: 'Escape' }), false)).toBe(true);
    expect(endsTransform(key({ key: 'Escape' }), true)).toBe(false);
    expect(endsTransform(key({ key: 'Enter' }), false)).toBe(false);
  });

  it('leaves every key to an open modal dialog', () => {
    const dialog = stubDialog();
    dialog.open = true;
    expect(shadowKeyStep(key({ code: 'NumpadAdd', ctrlKey: true }), true)).toBe(0);
    expect(startsTwiddle(key({ key: 'w' }), true)).toBe(false);
    expect(endsTransform(key({ key: 'Escape' }), false)).toBe(false);
  });

  it('knows a text field from other targets', () => {
    expect(typingIn(null)).toBe(false);
    expect(typingIn(element({ tagName: 'TEXTAREA' }))).toBe(true);
    expect(typingIn(element({ tagName: 'DIV', isContentEditable: true }))).toBe(true);
    expect(typingIn(element({ tagName: 'BUTTON' }))).toBe(false);
  });
});
