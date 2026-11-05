import { describe, expect, it } from 'vitest';

import {
  burnsAFile,
  convertLaserPictureUnits,
  initialLaserPictureState,
  laserPictureParams,
  laserPictureStateFrom,
  lineSpacingRange,
  validateLaserPicture,
} from './model';

describe('laser picture form', () => {
  it('burns the selected bitmap as a raster at the tool power by default', () => {
    const params = laserPictureParams(initialLaserPictureState('mm'), 'mm');
    expect(params).toMatchObject({
      strategy: 'raster',
      lineAngle: 0,
      repeats: 1,
      mode: 'powerModulation',
      maxPower: null,
      speed: null,
      lineInterval: 0.1,
      path: '',
    });
    if (params !== null) {
      expect(burnsAFile(params)).toBe(false);
    }
  });

  it('keeps the line angle between -90 and 90 and the power a percentage', () => {
    const state = initialLaserPictureState('mm');
    expect(validateLaserPicture({ ...state, lineAngle: '95' }, 'mm').lineAngle).toBeDefined();
    expect(validateLaserPicture({ ...state, lineAngle: '-90' }, 'mm').lineAngle).toBeUndefined();
    expect(validateLaserPicture({ ...state, power: '101' }, 'mm').power).toBeDefined();
    expect(validateLaserPicture({ ...state, repeats: '0' }, 'mm').repeats).toBeDefined();
    expect(laserPictureParams({ ...state, lineAngle: '95' }, 'mm')).toBeNull();
  });

  it('reads a toolpath back and converts units', () => {
    const state = {
      ...initialLaserPictureState('mm'),
      strategy: 'hatch' as const,
      ditherImage: true,
      lineAngle: '45',
      power: '60',
      repeats: '2',
      overscan: '5',
    };
    const params = laserPictureParams(state, 'mm');
    expect(params).toMatchObject({ mode: 'dither', maxPower: 60, overscanPercent: 5 });
    if (params === null) {
      throw new Error('valid');
    }
    expect(laserPictureStateFrom(params, 'P', 'mm')).toMatchObject({
      ...state,
      name: 'P',
      lineSpacing: '0.1',
    });
    const inch = convertLaserPictureUnits({ ...state, lineSpacing: '0.254' }, 'mm', 'inch');
    expect(Number(inch.lineSpacing)).toBeCloseTo(0.01);
    const range = lineSpacingRange('mm');
    expect(range.min).toBeLessThan(0.1);
    expect(range.max).toBeGreaterThan(0.1);
  });

  it('keeps the image file of an older toolpath', () => {
    const fresh = laserPictureParams(initialLaserPictureState('mm'), 'mm');
    if (fresh === null) {
      throw new Error('valid');
    }
    const old = {
      ...fresh,
      path: '/img.png',
      origin: { x: 5, y: 6 },
      width: 50,
      height: 0,
    };
    const state = laserPictureStateFrom(old, 'Old', 'mm');
    expect(state.legacy?.path).toBe('/img.png');
    expect(laserPictureParams(state, 'mm')).toMatchObject({
      path: '/img.png',
      origin: { x: 5, y: 6 },
      width: 50,
    });
  });
});
