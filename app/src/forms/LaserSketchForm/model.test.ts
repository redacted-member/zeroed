import { describe, expect, it } from 'vitest';

import {
  convertLaserSketchUnits,
  initialLaserSketchState,
  laserSketchParams,
  laserSketchStateFrom,
  validateLaserSketch,
} from './model';

function placed() {
  const state = initialLaserSketchState('mm');
  return { ...state, source: { ...state.source, bitmap: 9 } };
}

describe('laser sketch form', () => {
  it('uses the tool power when the field is empty and fills together', () => {
    expect(laserSketchParams(placed(), 'mm', 0)).toMatchObject({
      bitmap: 9,
      power: null,
      speed: null,
      stepover: 0.1,
      angle: 0,
      crossHatch: false,
      fill: 'together',
    });
  });

  it('refuses powers outside 0 to 100, a tiny stepover and negative overscan', () => {
    const state = { ...placed(), power: '150', stepover: '0.001', overscan: '-1' };
    const errors = validateLaserSketch(state, 'mm', 0);
    expect(errors.power).toBeDefined();
    expect(errors.stepover).toBeDefined();
    expect(errors.overscan).toBeDefined();
    expect(laserSketchParams(state, 'mm', 0)).toBeNull();
  });

  it('reads its settings back', () => {
    const state = {
      ...placed(),
      power: '35',
      crossHatch: true,
      fill: 'together' as const,
      angle: '45',
    };
    const params = laserSketchParams(state, 'mm', 0);
    expect(params).not.toBeNull();
    if (params !== null) {
      const back = laserSketchStateFrom(params, 'Burn', 'mm');
      expect(back).toMatchObject({
        name: 'Burn',
        power: '35',
        crossHatch: true,
        fill: 'together',
        angle: '45',
      });
      expect(back.source.bitmap).toBe(9);
    }
  });

  it('converts the stepover with the job unit', () => {
    const inch = convertLaserSketchUnits(initialLaserSketchState('mm'), 'mm', 'inch');
    expect(Number(inch.stepover)).toBeCloseTo(0.0039, 3);
  });
});
