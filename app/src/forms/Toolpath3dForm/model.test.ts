import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import {
  angleFor,
  finish3dParams,
  minimumDetailRange,
  initial3dState,
  needsLevel,
  rasterDirection,
  rough3dParams,
  state3dFrom,
  usesAngle,
  usesProfile,
  validate3d,
} from './model';

describe('3D toolpath form model', () => {
  it('finishes over the model with offset passes and the tool stepover by default', () => {
    expect(finish3dParams(initial3dState('finish3d', 'mm'), 'mm')).toEqual({
      stepover: null,
      angle: 0,
      allowance: 0,
      boundary: 'model',
      level: 0,
      boundaryOffset: 0,
      strategy: 'offset',
      direction: 'climb',
      stepoverRetract: 0,
      restTools: [],
      minimumDetail: 0.1,
      restBoundaryOffset: 1,
    });
  });

  it('keeps a list of later tools that rest machine after the first', () => {
    const ball = (id: number, diameter: number): Tool => ({
      id,
      name: `Ball ${diameter}`,
      number: id,
      toolType: { kind: 'ballNose', diameter },
      passDepth: 1,
      stepover: diameter * 0.1,
      stepoverPercent: 10,
      feedRate: 1000,
      plungeRate: 300,
      feedUnits: 'mm/min',
      spindleSpeed: 18000,
      notes: '',
      units: 'mm',
      clearance: 0,
      cuttingData: [],
    });
    const state = { ...initial3dState('finish3d', 'mm'), restTools: [ball(3, 1), ball(2, 3)] };
    const params = finish3dParams({ ...state, minimumDetail: '0.2', restOffset: '2' }, 'mm');
    expect(params?.restTools?.map((t) => t.id)).toEqual([3, 2]);
    expect(params?.minimumDetail).toBe(0.2);
    expect(params?.restBoundaryOffset).toBe(2);
    expect(
      validate3d('finish3d', { ...state, minimumDetail: '-1' }, 'mm').minimumDetail,
    ).toBeDefined();
    expect(
      validate3d('finish3d', { ...initial3dState('finish3d', 'mm'), minimumDetail: '-1' }, 'mm'),
    ).toEqual({});
    if (params === null) {
      throw new Error('the settings are valid');
    }
    const back = state3dFrom('finish3d', params, 'F', 'inch');
    expect(back.restTools).toHaveLength(2);
    expect(Number(back.restOffset)).toBeCloseTo(2 / 25.4, 3);
  });

  it('sets Minimum Detail on a Coarse to Fine range around its default', () => {
    const mm = minimumDetailRange('mm');
    expect(mm.min).toBeLessThan(0.1);
    expect(mm.max).toBeGreaterThan(0.1);
    const inch = minimumDetailRange('inch');
    expect(Number(initial3dState('finish3d', 'inch').minimumDetail)).toBeGreaterThan(inch.min);
    expect(Number(initial3dState('finish3d', 'inch').minimumDetail)).toBeLessThan(inch.max);
  });

  it('roughs with an allowance and the tool pass depth', () => {
    const state = initial3dState('rough3d', 'mm');
    expect(rough3dParams(state, 'mm')).toEqual({
      stepover: null,
      angle: 0,
      allowance: 1,
      boundary: 'model',
      level: 0,
      boundaryOffset: 0,
      passDepth: null,
      strategy: 'zLevelRaster',
      profileEachLevel: false,
      profile: 'last',
      order: 'levelByLevel',
      reverseStep: false,
      avoidMachined: false,
      ramp: { enabled: false, distance: 20 },
    });
  });

  it('round trips roughing parameters', () => {
    const params = rough3dParams(
      { ...initial3dState('rough3d', 'inch'), boundary: 'material' },
      'inch',
    );
    expect(params).not.toBeNull();
    if (params !== null) {
      expect(rough3dParams(state3dFrom('rough3d', params, 'R', 'inch'), 'inch')).toEqual(params);
    }
  });

  it('ignores the raster angle for the offset strategies', () => {
    const finish = {
      ...initial3dState('finish3d', 'mm'),
      finishStrategy: 'raster' as const,
      angle: 'x',
    };
    expect(usesAngle('finish3d', finish)).toBe(true);
    expect(validate3d('finish3d', finish, 'mm').angle).toBeDefined();
    const offset = { ...finish, finishStrategy: 'offset' as const };
    expect(usesAngle('finish3d', offset)).toBe(false);
    expect(validate3d('finish3d', offset, 'mm')).toEqual({});
    expect(finish3dParams(offset, 'mm')?.strategy).toBe('offset');

    const rough = initial3dState('rough3d', 'mm');
    expect(usesAngle('rough3d', rough)).toBe(true);
    expect(usesAngle('rough3d', { ...rough, roughStrategy: 'raster3d' })).toBe(true);
  });

  it('profiles each level only with Z Level roughing', () => {
    const zLevel = { ...initial3dState('rough3d', 'mm'), profile: 'first' as const };
    expect(usesProfile(zLevel)).toBe(true);
    expect(rough3dParams(zLevel, 'mm')?.profile).toBe('first');
    const raster = { ...zLevel, roughStrategy: 'raster3d' as const };
    expect(usesProfile(raster)).toBe(false);
    expect(rough3dParams(raster, 'mm')).toMatchObject({ strategy: 'raster3d', profile: 'none' });
  });

  it('reads the old profile each level flag as a profile cut last', () => {
    const old = {
      stepover: null,
      angle: 0,
      allowance: 0.5,
      boundary: 'model' as const,
      passDepth: null,
      strategy: 'zLevelRaster' as const,
      profileEachLevel: true,
    };
    expect(state3dFrom('rough3d', old, 'R', 'mm').profile).toBe('last');
  });

  it('keeps Z level options off for 3D raster roughing', () => {
    const state = {
      ...initial3dState('rough3d', 'mm'),
      roughStrategy: 'raster3d' as const,
      order: 'depthFirst' as const,
      ramp: true,
      avoidMachined: true,
    };
    expect(rough3dParams(state, 'mm')).toMatchObject({
      strategy: 'raster3d',
      order: 'levelByLevel',
      avoidMachined: true,
      ramp: { enabled: false },
    });
    expect(rough3dParams({ ...state, roughStrategy: 'zLevelRaster' }, 'mm')).toMatchObject({
      order: 'depthFirst',
      avoidMachined: false,
      ramp: { enabled: true, distance: 20 },
    });
    expect(
      validate3d('rough3d', { ...state, roughStrategy: 'zLevelRaster', rampDistance: '0' }, 'mm')
        .rampDistance,
    ).toBeDefined();
  });

  it('maps the raster direction to an angle', () => {
    const state = initial3dState('rough3d', 'mm');
    expect(rasterDirection(state)).toBe('x');
    expect(rasterDirection({ ...state, angle: angleFor('y') })).toBe('y');
    expect(rasterDirection({ ...state, angle: '100' })).toBe('y');
    expect(rough3dParams({ ...state, angle: angleFor('y') }, 'mm')?.angle).toBe(90);
  });

  it('needs a level for a level boundary and sends the offset', () => {
    const state = { ...initial3dState('finish3d', 'mm'), boundary: 'level' as const };
    expect(needsLevel(state)).toBe(true);
    expect(finish3dParams(state, 'mm')).toBeNull();
    const chosen = { ...state, level: 4, boundaryOffset: '-2' };
    expect(finish3dParams(chosen, 'mm')).toMatchObject({
      boundary: 'level',
      level: 4,
      boundaryOffset: -2,
    });
    expect(finish3dParams({ ...chosen, boundary: 'model' }, 'mm')?.level).toBe(0);
  });

  it('round trips finishing direction and stepover retract', () => {
    const params = finish3dParams(
      {
        ...initial3dState('finish3d', 'inch'),
        finishStrategy: 'offset',
        direction: 'conventional',
        stepoverRetract: '0.1',
      },
      'inch',
    );
    expect(params?.stepoverRetract).toBeCloseTo(2.54);
    if (params !== null) {
      expect(finish3dParams(state3dFrom('finish3d', params, 'F', 'inch'), 'inch')).toEqual(params);
    }
    const bad = { ...initial3dState('finish3d', 'mm'), finishStrategy: 'offset' as const };
    expect(
      validate3d('finish3d', { ...bad, stepoverRetract: '-1' }, 'mm').stepoverRetract,
    ).toBeDefined();
  });

  it('round trips the strategies and opens the old extra ones on the nearest', () => {
    const finish = finish3dParams(
      { ...initial3dState('finish3d', 'mm'), finishStrategy: 'raster', angle: '30' },
      'mm',
    );
    expect(finish).not.toBeNull();
    if (finish !== null) {
      const state = state3dFrom('finish3d', finish, 'F', 'mm');
      expect(state.finishStrategy).toBe('raster');
      expect(finish3dParams(state, 'mm')).toEqual(finish);
      const old = state3dFrom('finish3d', { ...finish, strategy: 'rasterXY' }, 'F', 'mm');
      expect(old.finishStrategy).toBe('raster');
    }
    const rough = rough3dParams(
      { ...initial3dState('rough3d', 'mm'), roughStrategy: 'raster3d' },
      'mm',
    );
    expect(rough).not.toBeNull();
    if (rough !== null) {
      expect(state3dFrom('rough3d', rough, 'R', 'mm').roughStrategy).toBe('raster3d');
      const old = state3dFrom('rough3d', { ...rough, strategy: 'zLevelOffset' }, 'R', 'mm');
      expect(old.roughStrategy).toBe('zLevelRaster');
    }
  });
});
