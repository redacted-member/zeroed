import { describe, expect, it } from 'vitest';

import {
  MAX_SLIDER_ANGLE,
  draftAngle,
  initialAddDraftState,
  sliderAngle,
  validateAddDraft,
} from './model';

describe('add draft form model', () => {
  it('starts at ten degrees', () => {
    expect(draftAngle(initialAddDraftState())).toBe(10);
  });

  it('takes angles above 0 and below 90', () => {
    expect(draftAngle({ angle: '2.5' })).toBe(2.5);
    expect(draftAngle({ angle: '0' })).toBeNull();
    expect(draftAngle({ angle: '90' })).toBeNull();
    expect(validateAddDraft({ angle: 'x' }).angle).toBeDefined();
  });

  it('keeps the slider in its range', () => {
    expect(sliderAngle({ angle: '75' })).toBe(MAX_SLIDER_ANGLE);
    expect(sliderAngle({ angle: '12' })).toBe(12);
  });
});
