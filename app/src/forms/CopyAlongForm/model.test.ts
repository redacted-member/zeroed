import { describe, expect, it } from 'vitest';

import {
  copyAlongArgs,
  copyAlongReady,
  copyAlongRoles,
  initialCopyAlongState,
  validateCopyAlong,
} from './model';

describe('copy along form model', () => {
  it('copies the first selected object along every other vector', () => {
    expect(copyAlongRoles([4, 5, 6], 'object', 0)).toEqual({ objects: [4], curves: [5, 6] });
    expect(copyAlongRoles([4, 5], 'circles', 0)).toEqual({ objects: [], curves: [4, 5] });
    expect(copyAlongRoles([4, 5], 'object', 1)).toEqual({ objects: [], curves: [4, 5] });
    expect(copyAlongReady([], 'object', 2)).toBe(false);
    expect(copyAlongReady([1], 'object', 0)).toBe(false);
    expect(copyAlongReady([1], 'object', 1)).toBe(true);
    expect(copyAlongReady([1, 2], 'object', 0)).toBe(true);
    expect(copyAlongReady([1], 'circles', 0)).toBe(true);
  });

  it('places five copies following the curve by default', () => {
    expect(copyAlongArgs(initialCopyAlongState('mm'), 'mm', [7])).toEqual({
      pathIds: [7],
      distributeMode: 'count',
      count: 5,
      spacing: 20,
      forceEven: false,
      newLayer: false,
      alignToCurve: true,
      startOffset: 0,
      reverse: false,
      deleteOriginal: false,
    });
    const circles = { ...initialCopyAlongState('mm'), what: 'circles' as const };
    expect(copyAlongArgs(circles, 'mm', [7])?.circleDiameter).toBe(6);
    expect(validateCopyAlong({ ...circles, diameter: '0' }, 'mm').diameter).toBeDefined();
  });

  it('only checks the field the chosen mode uses', () => {
    const state = { ...initialCopyAlongState('mm'), count: 'x' };
    expect(validateCopyAlong(state, 'mm').count).toBeDefined();
    const bySpacing = { ...state, distributeMode: 'spacing' as const };
    expect(validateCopyAlong(bySpacing, 'mm')).toEqual({});
    expect(validateCopyAlong({ ...bySpacing, spacing: '0' }, 'mm').spacing).toBeDefined();
  });
});
