import { describe, expect, it } from 'vitest';

import { MATERIALS, materialLook } from './appearance';
import { dollyFactor, dragFor, sculptDragFor, sculptWheel } from './navigation';

describe('3D navigation schemes', () => {
  it('rotates with the left button and pans with Shift or the middle button in both', () => {
    for (const scheme of ['modern', 'legacy'] as const) {
      expect(dragFor(0, false, scheme)).toBe('orbit');
      expect(dragFor(0, true, scheme)).toBe('pan');
      expect(dragFor(1, false, scheme)).toBe('pan');
      expect(dragFor(4, false, scheme)).toBeNull();
    }
  });

  it('pans with the right button in the modern scheme and zooms in the legacy one', () => {
    expect(dragFor(2, false, 'modern')).toBe('pan');
    expect(dragFor(2, false, 'legacy')).toBe('dolly');
  });

  it('zooms evenly both ways', () => {
    expect(dollyFactor(0)).toBe(1);
    expect(dollyFactor(50) * dollyFactor(-50)).toBeCloseTo(1, 12);
    expect(dollyFactor(10)).toBeGreaterThan(1);
  });
});

describe('material looks', () => {
  it('uses the theme and solid colours for those choices', () => {
    expect(materialLook('theme', '#111111', '#222222').colour).toBe('#111111');
    expect(materialLook('solid', '#111111', '#222222').colour).toBe('#222222');
  });

  it('gives every preset a colour and a finish in range', () => {
    for (const material of MATERIALS) {
      const look = materialLook(material, '#111111', '#222222');
      expect(look.colour).toMatch(/^#[0-9a-f]{6}$/);
      expect(look.roughness).toBeGreaterThanOrEqual(0);
      expect(look.roughness).toBeLessThanOrEqual(1);
      expect(look.metalness).toBeGreaterThanOrEqual(0);
      expect(look.metalness).toBeLessThanOrEqual(1);
    }
    expect(materialLook('aluminium', '#111111', '#222222').metalness).toBeGreaterThan(0.5);
  });

  it('turns the view with Alt or the right button while sculpting', () => {
    expect(sculptDragFor(0, false)).toBeNull();
    expect(sculptDragFor(0, true)).toBe('orbit');
    expect(sculptDragFor(0, false)).toBeNull();
    expect(sculptDragFor(1, false)).toBe('pan');
    expect(sculptDragFor(2, false)).toBe('orbit');
  });

  it('sizes the brush with the wheel while sculpting', () => {
    expect(sculptWheel({ alt: true, shift: false, ctrl: false })).toBe('zoom');
    expect(sculptWheel({ alt: false, shift: true, ctrl: false })).toBe('strength');
    expect(sculptWheel({ alt: false, shift: false, ctrl: true })).toBe('fineDiameter');
    expect(sculptWheel({ alt: false, shift: false, ctrl: false })).toBe('diameter');
  });
});
