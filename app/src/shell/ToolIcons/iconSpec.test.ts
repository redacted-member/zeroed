import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { EXTRA_ICONS } from './extraIcons';
import { ICON_FLAGS, flagClasses, iconRoles, iconShapes, unknownFlags } from './iconSpec';
import { drawIcon, iconSpec, loadExtraIcons } from './ToolIcons';

const read = (path: string) => readFileSync(fileURLToPath(new URL(path, import.meta.url)), 'utf8');

/** The `name: #hex` entries of one scheme map of _tokens.scss. */
function scheme(name: 'light' | 'dark'): Record<string, string> {
  const scss = read('../../styles/_tokens.scss');
  const body = new RegExp(`\\$${name}: \\(([^;]*?)\\n\\);`).exec(scss)?.[1] ?? '';
  return Object.fromEntries(
    [...body.matchAll(/^\s*([a-z0-9-]+): (#[0-9a-f]{6}),/gm)].map((m) => [m[1], m[2]]),
  );
}

function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = parseInt(hex.slice(1 + 2 * i, 3 + 2 * i), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2);
}

/** The WCAG contrast ratio of two colors. */
function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return ((hi ?? 0) + 0.05) / ((lo ?? 0) + 0.05);
}

const ROLES = Object.values(ICON_FLAGS).flatMap((f) => (f.kind === 'role' ? [f.token] : []));
const SURFACES = Object.values(ICON_FLAGS).flatMap((f) => (f.kind === 'surface' ? [f.token] : []));

describe('icon palette', () => {
  it('computes contrast as WCAG does', () => {
    expect(contrast('#000000', '#ffffff')).toBeCloseTo(21, 9);
    expect(contrast('#777777', '#ffffff')).toBeCloseTo(4.48, 2);
  });

  for (const name of ['light', 'dark'] as const) {
    const colors = scheme(name);
    it(`defines every role and surface in the ${name} scheme`, () => {
      expect([...ROLES, ...SURFACES].filter((token) => colors[token] === undefined)).toEqual([]);
    });

    it(`draws every role at 3:1 or more against the ${name} panels (WCAG non-text)`, () => {
      const low = ROLES.flatMap((role) =>
        ['panel-bg', 'panel-raised', 'panel-sunken'].flatMap((panel) => {
          const ratio = contrast(colors[role] ?? '#000000', colors[panel] ?? '#000000');
          return ratio < 3 ? [`${role} on ${panel}: ${ratio.toFixed(2)}`] : [];
        }),
      );
      expect(low).toEqual([]);
    });

    it(`keeps every role and the text at 3:1 or more on the ${name} surfaces`, () => {
      const low = [...ROLES, 'text'].flatMap((role) =>
        SURFACES.flatMap((surface) => {
          const ratio = contrast(colors[role] ?? '#000000', colors[surface] ?? '#000000');
          return ratio < 3 ? [`${role} on ${surface}: ${ratio.toFixed(2)}`] : [];
        }),
      );
      expect(low).toEqual([]);
    });
  }

  it('gives every role and surface a ToolIcons.scss class on its token', () => {
    const scss = read('./ToolIcons.scss');
    for (const flag of Object.values(ICON_FLAGS)) {
      if (flag.kind === 'style') {
        expect(scss).toContain(`.${flag.className} {`);
        continue;
      }
      const property = flag.kind === 'role' ? 'color' : 'fill';
      expect(scss).toContain(`.${flag.className} {\n  ${property}: tokens.$${flag.token};\n}`);
    }
  });

  it('turns every role and surface back to the text color in the mono mixin', () => {
    const scss = read('../../styles/_tokens.scss');
    const mixin = /@mixin tool-icon-mono \{([^}]*)\}/.exec(scss)?.[1] ?? '';
    for (const token of ROLES) {
      expect(mixin).toContain(`--${token}: currentColor;`);
    }
    for (const token of SURFACES) {
      expect(mixin).toContain(`--${token}: color-mix(in srgb, currentColor 25%, transparent);`);
    }
  });
});

describe('icon spec format', () => {
  it('splits shapes and their flags', () => {
    expect(iconShapes('M1 2;tV:c3 4 5;r1 1 2 2')).toEqual([
      { flags: '', body: 'M1 2' },
      { flags: 'tV', body: 'c3 4 5' },
      { flags: '', body: 'r1 1 2 2' },
    ]);
  });

  it('maps flags to classes, lower case for style, upper case for roles', () => {
    expect(flagClasses('')).toBeUndefined();
    expect(flagClasses('dt')).toBe('tool-icon__dash tool-icon__thin');
    expect(flagClasses('nV')).toBe('tool-icon__tint tool-icon__vector');
    expect(flagClasses('Ms')).toBe('tool-icon__model tool-icon__surface');
    expect(flagClasses('w')).toBe('tool-icon__wood');
    expect(flagClasses('x')).toBeUndefined();
    expect(unknownFlags('tV:M1 2;xq:c1 1 1;P:M0 0')).toEqual(['x', 'q']);
  });

  it('lists the roles a spec uses once each', () => {
    expect(iconRoles('V:M1 2;tV:c1 1 1;nR:M0 0;s:M0 0')).toEqual(['V', 'R']);
    expect(iconRoles('M1 2;t:c1 1 1')).toEqual([]);
  });

  it('draws the role and surface classes on the shapes', () => {
    const markup = renderToStaticMarkup(drawIcon('M1 2;fS:c3 4 5;Mw:r1 1 2 2'));
    expect(markup).toContain('<path d="M1 2"></path>');
    expect(markup).toContain(
      '<circle class="tool-icon__fill tool-icon__node" cx="3" cy="4" r="5">',
    );
    expect(markup).toContain('class="tool-icon__model tool-icon__wood"');
  });

  it('colors every icon with known flags, most of it neutral and at most two roles', async () => {
    await loadExtraIcons();
    const ids = [
      ...EXTRA_ICONS.keys(),
      ...[...read('./ToolIcons.tsx').matchAll(/\['(\w+)',/g)].map((match) => match[1] ?? ''),
    ];
    const specs = ids.flatMap((id) => {
      const spec = iconSpec(id);
      return spec === undefined ? [] : [[id, spec] as const];
    });
    expect(specs.length).toBeGreaterThan(300);
    expect(specs.filter(([, spec]) => unknownFlags(spec).length > 0).map(([id]) => id)).toEqual([]);
    expect(specs.filter(([, spec]) => iconRoles(spec).length > 2).map(([id]) => id)).toEqual([]);
    const colored = specs.filter(([, spec]) =>
      iconShapes(spec).some((shape) => /[A-Zsw]/.test(shape.flags)),
    );
    // Every tool family carries color; only plain list arrows, playback and
    // a few generic buttons stay neutral.
    expect(colored.length / specs.length).toBeGreaterThan(0.85);
    // Most of each icon stays neutral: no icon is all role color.
    const allColor = specs.filter(
      ([, spec]) =>
        iconShapes(spec).length > 2 && iconShapes(spec).every((s) => /[A-Z]/.test(s.flags)),
    );
    expect(allColor.map(([id]) => id)).toEqual([]);
  });
});
