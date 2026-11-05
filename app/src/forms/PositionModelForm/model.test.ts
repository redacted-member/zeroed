import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB } from '../../bridge/types';
import {
  canMakeBothSides,
  PLANE_STEPS,
  planeAtStep,
  planeError,
  planeText,
  planeValue,
  stepOfPlane,
} from './model';

describe('position model form', () => {
  it('keeps the zero plane inside the model height', () => {
    expect(planeValue('5', 'mm', 20)).toBe(5);
    expect(planeValue('25', 'mm', 20)).toBeNull();
    expect(planeError('-1', 'mm', 20)).toBeDefined();
    expect(planeValue('0.5', 'inch', 20)).toBeCloseTo(12.7);
    expect(planeText(12.7, 'inch')).toBe('0.5');
  });

  it('maps the slider to heights and back', () => {
    expect(planeAtStep(PLANE_STEPS / 2, 30)).toBe(15);
    expect(planeAtStep(PLANE_STEPS * 2, 30)).toBe(30);
    expect(stepOfPlane(15, 30)).toBe(PLANE_STEPS / 2);
    expect(stepOfPlane(3, 0)).toBe(0);
  });

  it('offers both sides only on a double sided job', () => {
    expect(canMakeBothSides(DEFAULT_JOB)).toBe(false);
    expect(canMakeBothSides({ ...DEFAULT_JOB, kind: 'doubleSided' })).toBe(true);
  });
});
