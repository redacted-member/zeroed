import { describe, expect, it } from 'vitest';

import { DEFAULT_SNAP } from '../../state/viewStore';
import {
  snapSettingsFromView,
  snapSettingsValues,
  toggleSnap,
  validateSnapSettings,
} from './model';

describe('snap settings model', () => {
  it('round trips the view values', () => {
    const state = snapSettingsFromView(DEFAULT_SNAP, 10, 12, 'mm');
    expect(state.gridSpacing).toBe('10');
    expect(state.snapRadiusPx).toBe('12');
    expect(snapSettingsValues(state, 'mm')).toEqual({
      snap: DEFAULT_SNAP,
      gridSpacingMm: 10,
      snapRadiusPx: 12,
      snapDistances: false,
      nudgeDistanceMm: 5,
    });
  });

  it('carries Snap Distances and the fixed nudge distance in the job unit', () => {
    const state = snapSettingsFromView(DEFAULT_SNAP, 25.4, 10, 'inch', true, 12.7);
    expect(state.nudgeDistance).toBe('0.5');
    const values = snapSettingsValues({ ...state, nudgeDistance: '0.25' }, 'inch');
    expect(values?.snapDistances).toBe(true);
    expect(values?.nudgeDistanceMm).toBeCloseTo(6.35);
    expect(
      validateSnapSettings({ ...state, nudgeDistance: '0' }, 'inch').nudgeDistance,
    ).toBeDefined();
    expect(
      validateSnapSettings({ ...state, snapRadiusPx: '51' }, 'inch').snapRadiusPx,
    ).toBeDefined();
  });

  it('converts inch grid spacing back to millimetres', () => {
    const state = snapSettingsFromView(DEFAULT_SNAP, 25.4, 10, 'inch');
    expect(state.gridSpacing).toBe('1');
    expect(snapSettingsValues(state, 'inch')?.gridSpacingMm).toBe(25.4);
  });

  it('toggles a snap option and validates the numbers', () => {
    const state = toggleSnap(snapSettingsFromView(DEFAULT_SNAP, 10, 10, 'mm'), 'grid');
    expect(state.snap.grid).toBe(!DEFAULT_SNAP.grid);
    expect(validateSnapSettings({ ...state, gridSpacing: '0' }, 'mm').gridSpacing).toBeDefined();
    expect(
      validateSnapSettings({ ...state, snapRadiusPx: '500' }, 'mm').snapRadiusPx,
    ).toBeDefined();
    expect(snapSettingsValues({ ...state, snapRadiusPx: 'x' }, 'mm')).toBeNull();
  });
});
