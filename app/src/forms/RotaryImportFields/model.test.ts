import { describe, expect, it } from 'vitest';

import {
  fitFromState,
  fitToBlank,
  originalSize,
  rotaryErrors,
  rotaryFitState,
  typeSize,
  withOffset,
} from './model';

const fit = { offset: [0, 0] as [number, number], alongScale: 1, radialScale: 1 };
const cylinder = { length: 100, diameter: 20 };

describe('rotary import fields', () => {
  it('shows the cylinder round the model and the axis offset', () => {
    const state = rotaryFitState({ ...fit, offset: [2, -3] }, cylinder, 'mm');
    expect(state).toEqual({ length: '100', diameter: '20', lockRatio: true, side: '2', up: '-3' });
  });

  it('keeps the ratio while it is locked', () => {
    const state = rotaryFitState(fit, cylinder, 'mm');
    expect(typeSize(state, 'length', '50', cylinder, 'mm')).toMatchObject({
      length: '50',
      diameter: '10',
    });
    const free = { ...state, lockRatio: false };
    expect(typeSize(free, 'diameter', '40', cylinder, 'mm')).toMatchObject({
      length: '100',
      diameter: '40',
    });
  });

  it('turns typed sizes into scales of the current fit', () => {
    const state = { ...rotaryFitState(fit, cylinder, 'mm'), length: '50', diameter: '30', up: '4' };
    expect(fitFromState(state, { ...fit, alongScale: 2 }, cylinder, 'mm')).toEqual({
      offset: [0, 4],
      alongScale: 1,
      radialScale: 1.5,
    });
    expect(rotaryErrors({ ...state, diameter: '0' }, 'mm').diameter).toBeDefined();
    expect(fitFromState({ ...state, diameter: '0' }, fit, cylinder, 'mm')).toBeNull();
  });

  it('fits the blank, keeping the ratio when locked, and resets the size', () => {
    const blank = { length: 300, diameter: 40 };
    expect(fitToBlank(fit, cylinder, blank, true)).toMatchObject({ alongScale: 2, radialScale: 2 });
    expect(fitToBlank(fit, cylinder, blank, false)).toMatchObject({
      alongScale: 3,
      radialScale: 2,
    });
    expect(originalSize({ ...fit, alongScale: 4, radialScale: 5 })).toEqual(fit);
  });

  it('fills the offset fields from a pan in the 3D view', () => {
    const state = rotaryFitState(fit, cylinder, 'mm');
    expect(withOffset(state, [1.5, -2], 'mm')).toMatchObject({ side: '1.5', up: '-2' });
  });
});
