import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import { ICON_PX, PALETTE_TOKENS, presentation, rootPresentation } from './iconStyle';

const PALETTE = {
  text: '#111111',
  'icon-vector': '#0000aa',
  'icon-result': '#aa0000',
  'icon-model-fill': '#eecc88',
};

describe('native menu icon style', () => {
  it('gives shapes the look of the ToolIcons line style classes', () => {
    expect(presentation('tool-icon__thin tool-icon__dash', PALETTE)).toEqual({
      'stroke-width': '1',
      'stroke-dasharray': '2 2.5',
    });
    expect(presentation('tool-icon__fill', PALETTE)).toEqual({
      fill: 'currentColor',
      stroke: 'none',
    });
    expect(presentation('tool-icon__tint', PALETTE)).toEqual({
      fill: 'currentColor',
      'fill-opacity': '0.3',
    });
    expect(presentation('', PALETTE)).toEqual({});
  });

  it('colors a role shape from the palette, its fills and tints too, and a surface its fill', () => {
    expect(presentation('tool-icon__vector', PALETTE)).toEqual({ color: '#0000aa' });
    expect(presentation('tool-icon__fill tool-icon__result', PALETTE)).toEqual({
      fill: 'currentColor',
      stroke: 'none',
      color: '#aa0000',
    });
    expect(presentation('tool-icon__surface', PALETTE)).toEqual({ fill: '#eecc88' });
    // A role the palette lacks leaves the shape neutral.
    expect(presentation('tool-icon__ok', PALETTE)).toEqual({});
  });

  it('draws the rest in the text color, with the stroke of .tool-icon', () => {
    const root = rootPresentation(PALETTE);
    expect(root['color']).toBe('#111111');
    expect(root['stroke']).toBe('currentColor');
    expect(root['fill']).toBe('none');
  });

  it('reads the text color and every role and surface token', () => {
    expect([...PALETTE_TOKENS].sort()).toEqual(
      [
        'text',
        'icon-vector',
        'icon-node',
        'icon-model',
        'icon-toolpath',
        'icon-result',
        'icon-ok',
        'icon-warning',
        'icon-model-fill',
        'icon-material-fill',
      ].sort(),
    );
  });

  it('matches the stroke widths and dashes of ToolIcons.scss', () => {
    const scss = readFileSync(
      fileURLToPath(new URL('../ToolIcons/ToolIcons.scss', import.meta.url)),
      'utf8',
    );
    expect(scss).toContain(`stroke-width: ${rootPresentation(PALETTE)['stroke-width']};`);
    expect(scss).toContain(
      `stroke-width: ${presentation('tool-icon__thin', PALETTE)['stroke-width']};`,
    );
    expect(scss).toContain(
      `stroke-dasharray: ${presentation('tool-icon__dash', PALETTE)['stroke-dasharray']};`,
    );
    expect(scss).toContain(
      `fill-opacity: ${presentation('tool-icon__tint', PALETTE)['fill-opacity']};`,
    );
  });

  it('renders at the size the shell expects', () => {
    const rust = readFileSync(
      fileURLToPath(new URL('../../../src-tauri/src/menu.rs', import.meta.url)),
      'utf8',
    );
    expect(rust).toContain(`pub const ICON_PX: u32 = ${ICON_PX};`);
  });
});
