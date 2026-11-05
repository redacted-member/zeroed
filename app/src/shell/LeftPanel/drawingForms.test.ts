import { describe, expect, it } from 'vitest';

import { isDrawingForm } from './drawingForms';

describe('isDrawingForm', () => {
  it('knows the drawing tools of the Design tab', () => {
    expect(isDrawingForm('drawCircle')).toBe(true);
    expect(isDrawingForm('fillet')).toBe(true);
    expect(isDrawingForm('rotate')).toBe(true);
    expect(isDrawingForm('textSelection')).toBe(true);
  });

  it('leaves other forms alone', () => {
    expect(isDrawingForm(null)).toBe(false);
    expect(isDrawingForm('twoRailSweep')).toBe(false);
    expect(isDrawingForm('profileToolpath')).toBe(false);
  });
});
