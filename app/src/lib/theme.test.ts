import { afterEach, describe, expect, it, vi } from 'vitest';

import { legibleOn, readCanvasPalette } from './theme';

describe('2D view colour conventions', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('draws selections magenta with blue handles, open vectors red and guides cyan by default', () => {
    // No theme tokens set: the default colours apply.
    vi.stubGlobal('getComputedStyle', () => ({ getPropertyValue: () => '' }));
    const palette = readCanvasPalette({} as Element);
    expect(palette.selection).toBe('#e01ec8');
    expect(palette.handle).toBe('#1f6fd6');
    expect(palette.openVector).toBe('#d62828');
    expect(palette.snap).toBe('#d62828');
    expect(palette.guide).toBe('#00a5b8');
  });

  it('follows the theme tokens when they are set', () => {
    vi.stubGlobal('getComputedStyle', () => ({
      getPropertyValue: (name: string) => (name === '--canvas-selection' ? ' #ff00ff ' : ''),
    }));
    expect(readCanvasPalette({} as Element).selection).toBe('#ff00ff');
  });
});

describe('layer colours on the canvas', () => {
  it('keeps a layer colour that stands out from the background', () => {
    expect(legibleOn('#1f6fd6', '#ffffff', '#1c1c1c')).toBe('#1f6fd6');
    expect(legibleOn('#000000', '#ffffff', '#1c1c1c')).toBe('#000000');
  });

  it('swaps a layer colour that would vanish on a dark canvas', () => {
    expect(legibleOn('#000000', '#202328', '#eceef1')).toBe('#eceef1');
    expect(legibleOn('#111', '#202328', '#eceef1')).toBe('#eceef1');
  });

  it('leaves colours it cannot read alone', () => {
    expect(legibleOn('rebeccapurple', '#202328', '#eceef1')).toBe('rebeccapurple');
  });
});
