import { describe, expect, it } from 'vitest';

import type { StraightInlayParams } from '../../bridge/cam';
import {
  initialInlayProfileState,
  inlayProfileParams,
  inlayProfileStateFrom,
  validateInlayProfile,
} from './model';

describe('profile style inlay forms', () => {
  it('cuts an insert outside and a hole inside with the Profile options', () => {
    const state = initialInlayProfileState('mm', 'maleStraight');
    const withRamp = { ...state, profile: { ...state.profile, ramp: true, allowance: '0.25' } };
    const male = inlayProfileParams(withRamp, 'maleStraight', 'mm');
    expect(male).toMatchObject({ part: 'male', male: 'straight', allowance: 0.25 });
    expect(male?.profileOptions?.side).toBe('outside');
    expect(male?.profileOptions?.ramp.enabled).toBe(true);
    const hole = inlayProfileParams(
      initialInlayProfileState('mm', 'femaleHole'),
      'femaleHole',
      'mm',
    );
    expect(hole).toMatchObject({ part: 'female', female: 'hole' });
    expect(hole?.profileOptions?.side).toBe('inside');
  });

  it('needs a step that ends above the cut depth for a stepped insert', () => {
    const state = initialInlayProfileState('mm', 'maleStepped');
    expect(validateInlayProfile(state, 'maleStepped', 'mm')).toEqual({});
    const deep = { ...state, stepDepth: '50' };
    expect(validateInlayProfile(deep, 'maleStepped', 'mm').stepDepth).toBeDefined();
    expect(validateInlayProfile(deep, 'maleStraight', 'mm')).toEqual({});
    expect(inlayProfileParams({ ...state, stepWidth: '0' }, 'maleStepped', 'mm')).toBeNull();
  });

  it('reads a toolpath back, and an older one without Profile options', () => {
    const state = initialInlayProfileState('inch', 'maleStepped');
    const params = inlayProfileParams(
      { ...state, profile: { ...state.profile, leadKind: 'arc' } },
      'maleStepped',
      'inch',
    );
    expect(params).not.toBeNull();
    if (params === null) {
      return;
    }
    const back = inlayProfileStateFrom(params, 'Male Inlay 2', 'maleStepped', 'inch');
    expect(back.profile.name).toBe('Male Inlay 2');
    expect(back.profile.leadKind).toBe('arc');
    expect(inlayProfileParams(back, 'maleStepped', 'inch')).toEqual(params);
    const old: StraightInlayParams = {
      ...params,
      profileOptions: undefined,
      direction: 'conventional',
    };
    const legacy = inlayProfileStateFrom(old, 'Old', 'maleStepped', 'inch');
    expect(legacy.profile.direction).toBe('conventional');
    expect(legacy.profile.depths.cutDepth).toBe(back.profile.depths.cutDepth);
  });

  it('keeps tabs placed with Edit Tabs and their anchors, as Profile does', () => {
    const state = initialInlayProfileState('mm', 'femaleHole');
    const placed = {
      ...state,
      profile: {
        ...state.profile,
        tabs: true,
        tabPositions: [{ x: 10, y: 0 }],
        tabAnchors: [{ id: 7, loopIndex: 0, along: 0.25, at: { x: 10, y: 0 } }],
      },
    };
    const params = inlayProfileParams(placed, 'femaleHole', 'mm');
    expect(params?.profileOptions?.tabPositions).toEqual([{ x: 10, y: 0 }]);
    expect(params?.profileOptions?.tabAnchors).toHaveLength(1);
    if (params === null) {
      return;
    }
    const back = inlayProfileStateFrom(params, 'Hole', 'femaleHole', 'mm');
    expect(back.profile.tabPositions).toEqual([{ x: 10, y: 0 }]);
    expect(back.profile.tabAnchors).toEqual(placed.profile.tabAnchors);
  });
});
