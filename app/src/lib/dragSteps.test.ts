import { describe, expect, it } from 'vitest';

import { axisLocked, distanceStep, dragDelta, roundToStep } from './dragSteps';

describe('drag steps', () => {
  it('picks a 1, 2, 5 step of at least eight pixels for the zoom', () => {
    expect(distanceStep(0.1)).toBe(1);
    expect(distanceStep(0.2)).toBeCloseTo(2);
    expect(distanceStep(0.5)).toBe(5);
    expect(distanceStep(0.01)).toBeCloseTo(0.1);
    expect(distanceStep(3)).toBe(50);
  });

  it('rounds to the step and keeps an Alt drag on its longer axis', () => {
    expect(roundToStep(7.4, 5)).toBe(5);
    expect(roundToStep(-7.6, 5)).toBe(-10);
    expect(axisLocked(3, -8)).toEqual({ dx: 0, dy: -8 });
    expect(axisLocked(-9, 8)).toEqual({ dx: -9, dy: 0 });
    expect(dragDelta(12.3, 3.1, { alt: true, stepMm: 5 })).toEqual({ dx: 10, dy: 0 });
    expect(dragDelta(12.3, 3.1, { alt: false, stepMm: null })).toEqual({ dx: 12.3, dy: 3.1 });
  });
});
