import { describe, expect, it } from 'vitest';

import {
  MAX_PERCENT,
  convertScaleModelHeightUnits,
  exactArgs,
  initialScaleModelHeightState,
  percentArgs,
  percentOf,
  validateScaleModelHeight,
} from './model';

describe('scale model height form model', () => {
  it('starts at the full height with the model top as the exact height', () => {
    expect(initialScaleModelHeightState('mm', 12.5)).toEqual({ percent: 100, height: '12.5' });
    expect(initialScaleModelHeightState('mm', null).height).toBe('0');
  });

  it('scales by a whole percentage in range', () => {
    expect(percentArgs(49.6, undefined)).toEqual({ percent: 50 });
    expect(percentArgs(0, 4)).toEqual({ percent: 1, replace: 4 });
    expect(percentArgs(9000, undefined)).toEqual({ percent: MAX_PERCENT });
  });

  it('sets an exact height in millimetres', () => {
    expect(exactArgs({ percent: 100, height: '1' }, 'inch', 7)).toEqual({
      height: 25.4,
      replace: 7,
    });
    expect(exactArgs({ percent: 100, height: '0' }, 'mm', undefined)).toBeNull();
    expect(validateScaleModelHeight({ percent: 100, height: 'x' }, 'mm').height).toBeDefined();
  });

  it('converts the height with the job units and reads factors back', () => {
    expect(convertScaleModelHeightUnits({ percent: 80, height: '25.4' }, 'mm', 'inch')).toEqual({
      percent: 80,
      height: '1',
    });
    expect(percentOf(0.5)).toBe(50);
    expect(percentOf(10)).toBe(MAX_PERCENT);
  });

  it('scales both sides of a double sided job when asked', () => {
    expect(percentArgs(50, 3, true)).toEqual({ percent: 50, replace: 3, bothSides: true });
    expect(exactArgs({ percent: 100, height: '5' }, 'mm', undefined, true)).toEqual({
      height: 5,
      bothSides: true,
    });
  });
});
