import { describe, expect, it } from 'vitest';

import {
  convertVectorTextureUnits,
  initialVectorTextureState,
  validateVectorTexture,
  vectorTextureRequest,
} from './model';

describe('vector texture model', () => {
  it('starts with straight, even rows 5 mm apart', () => {
    const state = initialVectorTextureState('mm');
    expect(validateVectorTexture(state, 'mm')).toEqual({});
    expect(vectorTextureRequest(state, 'mm')).toEqual({
      params: {
        angle: 0,
        maxSpacing: 5,
        spacingVariation: 0,
        wavelength: 50,
        amplitude: 0,
        noise: 0,
      },
    });
  });

  it('sends lengths in millimetres from inch fields', () => {
    const state = { ...initialVectorTextureState('inch'), amplitude: '0.1' };
    const request = vectorTextureRequest(state, 'inch');
    expect(request?.params.maxSpacing).toBeCloseTo(5.08);
    expect(request?.params.amplitude).toBeCloseTo(2.54);
  });

  it('refuses an angle out of range and a zero spacing', () => {
    const state = { ...initialVectorTextureState('mm'), angle: '95', maxSpacing: '0' };
    const errors = validateVectorTexture(state, 'mm');
    expect(errors.angle).toBeDefined();
    expect(errors.maxSpacing).toBeDefined();
    expect(vectorTextureRequest(state, 'mm')).toBeNull();
  });

  it('names a layer only when asked', () => {
    const state = { ...initialVectorTextureState('mm'), layerName: 'Grain' };
    expect(vectorTextureRequest(state, 'mm')?.layerName).toBeUndefined();
    expect(vectorTextureRequest({ ...state, onLayer: true }, 'mm')?.layerName).toBe('Grain');
  });

  it('converts the lengths when the unit changes', () => {
    const inch = convertVectorTextureUnits(initialVectorTextureState('mm'), 'mm', 'inch');
    expect(Number(inch.wavelength)).toBeCloseTo(50 / 25.4, 2);
  });
});
