import { describe, expect, it } from 'vitest';

import {
  fitOptions,
  fitTolerance,
  fitToleranceError,
  initialFitCurvesState,
  maxAngleError,
} from './model';

describe('fit curves form model', () => {
  it('fits arcs to a tenth of a millimetre by default', () => {
    const state = initialFitCurvesState('mm');
    expect(state.kind).toBe('arcs');
    expect(fitTolerance(state, 'mm')).toBeCloseTo(0.1);
  });

  it('refuses a tolerance of zero', () => {
    const state = { ...initialFitCurvesState('mm'), tolerance: '0' };
    expect(fitToleranceError(state, 'mm')).toBeDefined();
    expect(fitTolerance(state, 'mm')).toBeNull();
  });

  it('sends Keep Sharp Corners and Replace selected vectors', () => {
    const state = initialFitCurvesState('mm');
    expect(fitOptions(state)).toEqual({ replace: true });
    const sharp = { ...state, keepSharp: true, replace: false };
    expect(fitOptions(sharp)).toEqual({ replace: false, sharpAngle: 20 });
    expect(maxAngleError({ ...sharp, maxAngle: '200' })).toBeDefined();
    expect(fitOptions({ ...sharp, maxAngle: 'x' })).toBeNull();
    expect(maxAngleError({ ...state, maxAngle: 'x' })).toBeUndefined();
  });
});
