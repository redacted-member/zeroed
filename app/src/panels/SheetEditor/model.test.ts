import { describe, expect, it } from 'vitest';

import type { SheetMaterial } from '../../bridge/document';
import { materialFromState, stateFromMaterial, validate } from './model';

const material: SheetMaterial = {
  width: 609.6,
  height: 304.8,
  thickness: 12.7,
  zZero: 'bottom',
  xyOrigin: 'centre',
  offsetX: 25.4,
  offsetY: 0,
};

describe('edit sheet form model', () => {
  it('shows the material in the job unit and gives it back in millimetres', () => {
    const inches = stateFromMaterial(material, 'inch');
    expect(inches.width).toBe('24');
    expect(inches.offsetX).toBe('1');
    expect(inches.zZero).toBe('bottom');
    const back = materialFromState(inches, 'inch');
    expect(back?.width).toBeCloseTo(609.6, 6);
    expect(back?.thickness).toBeCloseTo(12.7, 6);
    expect(back?.xyOrigin).toBe('centre');
    expect(materialFromState(stateFromMaterial(material, 'mm'), 'mm')).toEqual(material);
  });

  it('wants numbers, and a positive size and thickness', () => {
    const state = stateFromMaterial(material, 'mm');
    expect(validate(state)).toEqual({});
    expect(Object.keys(validate({ ...state, width: '0', offsetY: 'x' }))).toEqual([
      'width',
      'offsetY',
    ]);
    expect(validate({ ...state, offsetX: '-5' })).toEqual({});
    expect(materialFromState({ ...state, thickness: '' }, 'mm')).toBeNull();
    expect(materialFromState({ ...state, height: '100 * 2' }, 'mm')?.height).toBe(200);
  });
});
