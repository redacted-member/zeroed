import { describe, expect, it } from 'vitest';

import { strings } from '../../lib/strings';
import { flutingParams, flutingStateFrom, initialFlutingState, validateFluting } from './model';

describe('fluting toolpath form model', () => {
  it('flutes 3 mm deep with a linear ramp over the complete length by default', () => {
    expect(flutingParams(initialFlutingState('mm'), 'mm')).toEqual({
      selectionOrder: false,
      startDepth: 0,
      depth: 3,
      ramp: 'linear',
      rampStart: true,
      rampEnd: false,
      rampLength: null,
      rampPercent: null,
    });
  });

  it('maps the flute types to the ramped ends', () => {
    const base = initialFlutingState('mm');
    const complete = flutingParams({ ...base, fluteType: 'complete' }, 'mm');
    expect(complete).toMatchObject({ rampStart: true, rampEnd: false, rampLength: null });
    expect(complete?.rampPercent).toBeNull();
    const start = flutingParams({ ...base, fluteType: 'start', rampBy: 'length' }, 'mm');
    expect(start).toMatchObject({ rampStart: true, rampEnd: false, rampLength: 10 });
    expect(start?.rampPercent).toBeNull();
  });

  it('checks the depths and the ramp size in use', () => {
    const state = {
      ...initialFlutingState('mm'),
      fluteType: 'startEnd' as const,
      rampLength: '0',
      rampPercent: '120',
    };
    expect(validateFluting({ ...state, fluteType: 'complete' }, 'mm')).toEqual({});
    expect(validateFluting(state, 'mm').rampPercent).toBeDefined();
    expect(validateFluting({ ...state, rampBy: 'length' }, 'mm').rampLength).toBe(
      strings.form.errorPositive,
    );
    expect(validateFluting({ ...state, depth: '0' }, 'mm').depth).toBeDefined();
    expect(validateFluting({ ...state, startDepth: '-1' }, 'mm').startDepth).toBeDefined();
    expect(flutingParams({ ...state, depth: 'x' }, 'mm')).toBeNull();
  });

  it('converts inches and round trips', () => {
    const state = {
      ...initialFlutingState('inch'),
      startDepth: '0.1',
      ramp: 'linear' as const,
      fluteType: 'start' as const,
      rampBy: 'length' as const,
      rampLength: '1',
    };
    const params = flutingParams(state, 'inch');
    expect(params?.rampLength).toBeCloseTo(25.4);
    expect(params?.startDepth).toBeCloseTo(2.54);
    if (params !== null) {
      expect(flutingParams(flutingStateFrom(params, 'F', 'inch'), 'inch')).toEqual(params);
    }
  });

  it('loads toolpaths saved before start depth and flute types', () => {
    const old = { depth: 3, ramp: 'smooth' as const, rampStart: true, rampEnd: false };
    const loaded = flutingStateFrom({ ...old, rampLength: null }, 'Old', 'mm');
    expect(loaded.fluteType).toBe('complete');
    expect(loaded.startDepth).toBe('0');
    expect(flutingStateFrom({ ...old, rampLength: 5 }, 'Old', 'mm').fluteType).toBe('start');
  });

  it('sends Use Vector Selection Order and reads it back', () => {
    const state = { ...initialFlutingState('mm'), selectionOrder: true };
    const params = flutingParams(state, 'mm');
    expect(params?.selectionOrder).toBe(true);
    if (params !== null) {
      expect(flutingStateFrom(params, 'N', 'mm').selectionOrder).toBe(true);
      expect(
        flutingStateFrom({ ...params, selectionOrder: undefined }, 'N', 'mm').selectionOrder,
      ).toBe(false);
    }
  });
});
