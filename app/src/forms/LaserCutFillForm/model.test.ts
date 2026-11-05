import { describe, expect, it } from 'vitest';

import type { LaserCutFillParams } from '../../bridge/cam';

import {
  convertLaserCutFillUnits,
  initialLaserCutFillState,
  laserCutFillParams,
  laserCutFillStateFrom,
  laserPasses,
  validateLaserCutFill,
} from './model';

describe('laser cut and fill form', () => {
  it('uses the tool power and speed when the fields are empty', () => {
    const params = laserCutFillParams(initialLaserCutFillState('mm'), 'mm');
    expect(params).toMatchObject({
      mode: 'cut',
      side: 'outside',
      repeats: 1,
      cutDepth: 0,
      power: null,
      speed: null,
    });
    expect(params?.fillPower).toBeNull();
  });

  it('refuses powers outside 0 to 100 percent and zero repeats', () => {
    const state = { ...initialLaserCutFillState('mm'), power: '120', repeats: '0' };
    const errors = validateLaserCutFill(state, 'mm');
    expect(errors.power).toBeDefined();
    expect(errors.repeats).toBeDefined();
    expect(laserCutFillParams(state, 'mm')).toBeNull();
  });

  it('checks the fill settings only when filling', () => {
    const cut = { ...initialLaserCutFillState('mm'), lineInterval: '0' };
    expect(validateLaserCutFill(cut, 'mm').lineInterval).toBeUndefined();
    const fill = { ...cut, strategy: 'hatch' as const, outline: false, overscan: '5' };
    expect(validateLaserCutFill(fill, 'mm').lineInterval).toBeDefined();
    const params = laserCutFillParams({ ...fill, lineInterval: '0.2', fillRegions: true }, 'mm');
    expect(params).toMatchObject({
      mode: 'fill',
      lineInterval: 0.2,
      overscanPercent: 5,
      fillRegions: true,
    });
    expect(validateLaserCutFill({ ...fill, overscan: '150' }, 'mm').overscan).toBeDefined();
  });

  it('counts the passes from the cut depth and the laser pass depth', () => {
    expect(laserPasses(0, 1)).toBe(1);
    expect(laserPasses(3, 1)).toBe(3);
    expect(laserPasses(2.5, 1)).toBe(3);
    expect(laserPasses(2, 0)).toBe(1);
    const deep = laserCutFillParams({ ...initialLaserCutFillState('mm'), cutDepth: '3' }, 'mm');
    expect(deep?.cutDepth).toBe(3);
    // Older toolpaths kept the repeats as passes.
    const legacy: LaserCutFillParams = {
      mode: 'cut',
      side: 'on',
      passes: 4,
      power: null,
      speed: null,
      fillPower: null,
      lineInterval: 0.1,
      angle: 0,
      hatch: 'lines',
      bidirectional: true,
      overscan: 0,
    };
    const old = laserCutFillStateFrom(legacy, 'Old', 'mm');
    expect(old.repeats).toBe('4');
  });

  it('converts lengths between units and reads a toolpath back', () => {
    const inch = convertLaserCutFillUnits(
      { ...initialLaserCutFillState('mm'), speed: '2540', lineInterval: '0.254' },
      'mm',
      'inch',
    );
    expect(Number(inch.speed)).toBeCloseTo(100);
    expect(Number(inch.lineInterval)).toBeCloseTo(0.01);
    const params = laserCutFillParams({ ...inch, strategy: 'hatch', hatch: 'cross' }, 'inch');
    expect(params?.speed).toBeCloseTo(2540);
    if (params === null) {
      throw new Error('no params');
    }
    const back = laserCutFillStateFrom(params, 'Laser 1', 'mm');
    expect(back).toMatchObject({
      name: 'Laser 1',
      strategy: 'hatch',
      outline: true,
      hatch: 'cross',
    });
    expect(params.mode).toBe('cutAndFill');
    expect(Number(back.speed)).toBeCloseTo(2540);
  });

  it('maps the Strategy radio and sends the allowance only for outside and inside cuts', () => {
    const base = { ...initialLaserCutFillState('mm'), allowance: '0.3' };
    expect(laserCutFillParams({ ...base, strategy: 'inside' }, 'mm')).toMatchObject({
      mode: 'cut',
      side: 'inside',
      allowance: 0.3,
    });
    expect(laserCutFillParams({ ...base, strategy: 'on' }, 'mm')).toMatchObject({
      side: 'on',
      allowance: 0,
    });
    const hatch = laserCutFillParams({ ...base, strategy: 'hatch', outline: false }, 'mm');
    expect(hatch).toMatchObject({ mode: 'fill', side: 'on', allowance: 0 });
    if (hatch !== null) {
      expect(laserCutFillStateFrom(hatch, 'L', 'mm')).toMatchObject({
        strategy: 'hatch',
        outline: false,
      });
    }
  });
});
