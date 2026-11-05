import { describe, expect, it } from 'vitest';

import {
  convertPhotoUnits,
  initialPhotoState,
  LINE_SPACING,
  photoParams,
  photoStateFrom,
  validatePhoto,
} from './model';

describe('photo carve form model', () => {
  it('rasters at 45 degrees and 100 percent spacing by default', () => {
    expect(photoParams(initialPhotoState('mm'), 'mm')).toEqual({
      strategy: 'raster',
      lineAngle: 45,
      lineSpacing: 100,
      startDepth: 0,
      maxDepth: 2.5,
      stepoverRetract: 3,
    });
  });

  it('sends millimetres and reads a toolpath back', () => {
    const state = { ...initialPhotoState('inch'), maxDepth: '0.1', strategy: 'hatch' as const };
    const params = photoParams(state, 'inch');
    expect(params?.maxDepth).toBeCloseTo(2.54, 9);
    expect(params?.strategy).toBe('hatch');
    if (params === null) {
      throw new Error('the settings are valid');
    }
    const back = photoStateFrom({ ...params, lineAngle: 45 }, 'Portrait', 'mm');
    expect(back.maxDepth).toBe('2.54');
    expect(back.lineAngle).toBe(45);
    expect(convertPhotoUnits(back, 'mm', 'inch').maxDepth).toBe('0.1');
  });

  it('holds back a bad spacing or depth', () => {
    const state = { ...initialPhotoState('mm'), lineSpacing: '5', maxDepth: '0' };
    const errors = validatePhoto(state, 'mm');
    expect(errors.lineSpacing).toBeDefined();
    expect(errors.maxDepth).toBeDefined();
    expect(photoParams(state, 'mm')).toBeNull();
  });

  it('takes the Line Spacing the slider and box offer, from Dense to Sparse', () => {
    const at = (lineSpacing: number) =>
      validatePhoto({ ...initialPhotoState('mm'), lineSpacing: String(lineSpacing) }, 'mm')
        .lineSpacing;
    expect(at(LINE_SPACING.min)).toBeUndefined();
    expect(at(LINE_SPACING.max)).toBeUndefined();
    expect(at(LINE_SPACING.max + 1)).toBeDefined();
  });
});
