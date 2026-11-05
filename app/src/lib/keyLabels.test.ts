import { afterEach, describe, expect, it, vi } from 'vitest';

import { comboLabel, keys } from './keyLabels';

const stringModules = import.meta.glob<Record<string, unknown>>('./strings/*.ts');

/** Every text a value holds, calling functions with stand in arguments. */
function textsOf(value: unknown, out: string[] = []): string[] {
  if (typeof value === 'string') {
    out.push(value);
  } else if (typeof value === 'function') {
    const fn = value as (...args: unknown[]) => unknown;
    for (const stand of [1, 'x']) {
      try {
        textsOf(fn(...Array.from({ length: Math.max(fn.length, 1) }, () => stand)), out);
      } catch {
        // A function that wants other arguments shows no key names itself.
      }
    }
  } else if (value !== null && typeof value === 'object') {
    for (const item of Object.values(value)) {
      textsOf(item, out);
    }
  }
  return out;
}

/** Every text of lib/strings, loaded afresh as on `platform`. */
async function stringsOn(platform: string): Promise<string[]> {
  vi.stubGlobal('navigator', { platform });
  vi.resetModules();
  const texts: string[] = [];
  for (const [path, load] of Object.entries(stringModules)) {
    if (!path.endsWith('.test.ts')) {
      textsOf(await load(), texts);
    }
  }
  return texts;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
});

describe('key labels', () => {
  it('writes the Mac symbols in the Mac order, the names elsewhere', () => {
    const redo = { key: 'z', ctrl: true, shift: true };
    expect(comboLabel(redo, true)).toBe('⇧⌘Z');
    expect(comboLabel(redo, false)).toBe('Ctrl+Shift+Z');
    expect(comboLabel({ key: 'v', ctrl: true, shift: true, alt: true }, true)).toBe('⌥⇧⌘V');
    expect(comboLabel({ key: 'F3' }, true)).toBe('F3');
    expect(comboLabel({ key: 'PageUp' }, false, { PageUp: 'PgUp' })).toBe('PgUp');
    expect(keys('Ctrl+F', true)).toBe('⌘F');
    expect(keys('Ctrl+F', false)).toBe('Ctrl+F');
    expect(keys('Ctrl', true)).toBe('⌘');
    expect(keys('Alt', false)).toBe('Alt');
    expect(keys('Ctrl+Shift+arrow', true)).toBe('⇧⌘ arrow');
    expect(keys('Ctrl+Shift+arrow', false)).toBe('Ctrl+Shift+arrow');
    expect(keys('Control+C', true)).toBe('⌃C');
    // A plus as the key itself stays the key.
    expect(keys('Ctrl++', false)).toBe('Ctrl++');
  });

  it('shows no Windows key names in any interface text on the Mac', async () => {
    const texts = await stringsOn('MacIntel');
    expect(texts.length).toBeGreaterThan(1000);
    const windows = texts.filter((text) => /Ctrl\+|\bCtrl\b|\bAlt\b|\bCmd\b/.test(text));
    expect(windows).toEqual([]);
    expect(texts).toContain('Zoom To Selection (⌘F)');
  });

  it('keeps the key names on other platforms', async () => {
    const texts = await stringsOn('Win32');
    expect(texts).toContain('Zoom To Selection (Ctrl+F)');
    expect(texts.some((text) => text.includes('⌘'))).toBe(false);
  });
});
