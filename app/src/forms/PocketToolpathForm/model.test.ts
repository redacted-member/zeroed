import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import {
  initialPocketState,
  pocketOnRotary,
  pocketParams,
  pocketStateFrom,
  validatePocket,
  withLegacyClearance,
} from './model';

function tool(id: number, diameter: number, fromDatabase = true): Tool {
  return {
    id,
    name: `End mill ${diameter}`,
    number: id,
    ...(fromDatabase ? { diameter } : {}),
    toolType: { kind: 'endMill', diameter },
    passDepth: 3,
    stepover: diameter * 0.4,
    stepoverPercent: 40,
    feedRate: 1000,
    plungeRate: 300,
    feedUnits: 'mm/min',
    spindleSpeed: 18000,
    notes: '',
    units: 'mm',
    clearance: 0,
    cuttingData: [],
  };
}

describe('pocket toolpath form model', () => {
  it('clears with offset rings using the tool stepover by default', () => {
    expect(pocketParams(initialPocketState('mm'), 'mm')).toMatchObject({
      cutDepth: 6,
      stepover: null,
      strategy: 'offset',
      profilePass: true,
      clearanceTool: null,
      restTools: [],
    });
  });

  it('sends the tools after the first in cutting order and reads them back', () => {
    const later = [tool(2, 3), tool(3, 1)];
    const params = pocketParams({ ...initialPocketState('mm'), laterTools: later }, 'mm');
    expect(params?.restTools).toEqual(later);
    expect(params?.clearanceTool).toBeNull();
    if (params !== null) {
      expect(pocketStateFrom(params, 'P', 'mm').laterTools).toEqual(later);
    }
  });

  it('puts the clearance tool of an older two tool pocket first in the list', () => {
    const big = tool(2, 12);
    const main = tool(1, 6);
    const params = pocketParams(initialPocketState('mm'), 'mm');
    expect(params).not.toBeNull();
    if (params !== null) {
      const loaded = pocketStateFrom({ ...params, clearanceTool: big }, 'P', 'mm');
      expect(loaded.legacyClearance).toBe(big);
      const next = withLegacyClearance(loaded, main);
      expect(next.first).toBe(big);
      expect(next.state.laterTools).toEqual([main]);
      expect(next.state.legacyClearance).toBeNull();
      expect(pocketParams(next.state, 'mm')?.clearanceTool).toBeNull();
      const plain = withLegacyClearance(initialPocketState('mm'), main);
      expect(plain.first).toBe(main);
      expect(plain.state.laterTools).toEqual([]);
    }
  });

  it('loads toolpaths saved before the tool list existed', () => {
    const params = pocketParams(initialPocketState('mm'), 'mm');
    if (params !== null) {
      const old: Partial<typeof params> = { ...params };
      delete old.clearanceTool;
      delete old.restTools;
      const state = pocketStateFrom(old as typeof params, 'P', 'mm');
      expect(state.laterTools).toEqual([]);
      expect(state.legacyClearance).toBeNull();
    }
  });

  it('checks the raster angle only for raster clearing', () => {
    const state = { ...initialPocketState('mm'), rasterAngle: 'steep' };
    expect(validatePocket(state, 'mm').rasterAngle).toBeUndefined();
    expect(validatePocket({ ...state, strategy: 'raster' }, 'mm').rasterAngle).toBeDefined();
  });

  it('takes a negative allowance, limits the raster angle and round trips', () => {
    expect(
      validatePocket({ ...initialPocketState('mm'), allowance: '-1' }, 'mm').allowance,
    ).toBeUndefined();
    const raster = { ...initialPocketState('mm'), strategy: 'raster' as const };
    expect(validatePocket({ ...raster, rasterAngle: '120' }, 'mm').rasterAngle).toBeDefined();
    expect(validatePocket({ ...raster, rasterAngle: '90' }, 'mm').rasterAngle).toBeUndefined();
    const params = pocketParams(
      { ...initialPocketState('mm'), strategy: 'raster', rasterAngle: '45' },
      'mm',
    );
    expect(params).not.toBeNull();
    if (params !== null) {
      expect(pocketParams(pocketStateFrom(params, 'P', 'mm'), 'mm')).toEqual(params);
    }
  });

  it('sends the profile pass choice, pass options and selection order', () => {
    const state = {
      ...initialPocketState('mm'),
      strategy: 'raster' as const,
      profilePass: 'first' as const,
      selectionOrder: true,
      passes: {
        byCount: true,
        passCount: '3',
        useLastPass: false,
        lastPassThickness: '0.5',
        exact: false,
        list: null,
      },
    };
    const params = pocketParams(state, 'mm');
    expect(params).toMatchObject({
      profilePass: true,
      profilePassWhen: 'first',
      selectionOrder: true,
      passCount: 3,
      lastPassThickness: null,
    });
    expect(pocketParams({ ...state, profilePass: 'none' }, 'mm')?.profilePass).toBe(false);
    if (params !== null) {
      const back = pocketStateFrom(params, 'P', 'mm');
      expect(back.profilePass).toBe('first');
      expect(back.passes.byCount).toBe(true);
      expect(pocketParams(back, 'mm')).toEqual(params);
    }
  });

  it('opens a pocket saved before the profile pass choice as before', () => {
    const old = pocketParams(initialPocketState('mm'), 'mm');
    expect(old).not.toBeNull();
    if (old !== null) {
      const rest = { ...old };
      delete rest.profilePassWhen;
      delete rest.selectionOrder;
      expect(pocketStateFrom({ ...rest, profilePass: true }, 'P', 'mm').profilePass).toBe('last');
      expect(pocketStateFrom({ ...rest, profilePass: false }, 'P', 'mm').profilePass).toBe('none');
    }
  });
  it('sends inch lengths to the kernel in millimeters', () => {
    const state = {
      ...initialPocketState('inch'),
      allowance: '0.02',
      ramp: true,
      rampDistance: '1',
    };
    const params = pocketParams(state, 'inch');
    expect(params?.allowance).toBeCloseTo(0.508, 9);
    expect(params?.ramp.enabled).toBe(true);
    expect(params?.ramp.distance).toBeCloseTo(25.4, 9);
    expect(params?.cutDepth).toBeCloseTo(6.35, 9);
  });

  it('starts a rotary pocket as a raster along the blank with no profile pass', () => {
    const fresh = pocketOnRotary(initialPocketState('inch'), 90, false);
    expect(fresh.strategy).toBe('raster');
    expect(fresh.rasterAngle).toBe('90');
    expect(fresh.profilePass).toBe('none');
  });

  it('keeps the profile pass of a rotary pocket being edited', () => {
    const saved = { ...initialPocketState('inch'), profilePass: 'first' as const };
    expect(pocketOnRotary(saved, 90, true).profilePass).toBe('first');
  });
});
