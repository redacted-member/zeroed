import { describe, expect, it } from 'vitest';

import {
  initialPictureState,
  isNeutral,
  pictureRequest,
  borderWeight,
  editPixels,
  toneOf,
} from './model';

describe('edit picture model', () => {
  it('starts neutral and asks for nothing', () => {
    const state = initialPictureState();
    expect(isNeutral(state)).toBe(true);
    expect(pictureRequest(state, 4)).toBeNull();
  });

  it('needs a bitmap', () => {
    expect(pictureRequest({ ...initialPictureState(), invert: true }, null)).toBeNull();
  });

  it('builds the request for the selected bitmap', () => {
    const state = { ...initialPictureState(), contrast: 30, border: 'oval' as const, fade: 80 };
    expect(pictureRequest(state, 7)).toEqual({
      id: 7,
      contrast: 30,
      brightness: 0,
      gamma: 1,
      invert: false,
      greyscale: false,
      border: 'oval',
      fade: 50,
    });
  });

  it('counts a border as a change', () => {
    expect(isNeutral({ ...initialPictureState(), border: 'rectangle' })).toBe(false);
  });
});

describe('the live preview matches the kernel edit', () => {
  it('maps tones as the kernel does', () => {
    const state = initialPictureState();
    expect(toneOf(state, 0.25)).toBeCloseTo(0.25);
    expect(toneOf({ ...state, invert: true }, 0.25)).toBeCloseTo(0.75);
    expect(toneOf({ ...state, contrast: 100 }, 0.6)).toBeCloseTo(1);
    expect(toneOf({ ...state, brightness: 50 }, 0.2)).toBeCloseTo(0.7);
  });

  it('greys, and fades a border to white', () => {
    const px = new Uint8ClampedArray([255, 0, 0, 255]);
    editPixels(px, 1, 1, { ...initialPictureState(), greyscale: true });
    expect([px[0], px[1], px[2]]).toEqual([76, 76, 76]);
    const w = { ...initialPictureState(), border: 'rectangle' as const, fade: 50 };
    expect(borderWeight(w, 0.1, 5, 10, 10)).toBeLessThan(0.1);
    expect(borderWeight(w, 5, 5, 10, 10)).toBe(1);
  });
});
