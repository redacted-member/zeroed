import { describe, expect, it } from 'vitest';

import { reversedPosition, sliderPosition, typedValue } from './model';

describe('SliderBoxField', () => {
  it('keeps the slider inside its range', () => {
    expect(sliderPosition(50, 0, 100)).toBe(50);
    expect(sliderPosition(-5, 0, 100)).toBe(0);
    expect(sliderPosition(500, 0, 100)).toBe(100);
    expect(sliderPosition(Number.NaN, 10, 100)).toBe(10);
  });

  it('takes a typed number only inside the range', () => {
    expect(typedValue('40', 0, 100)).toBe(40);
    expect(typedValue('12.5', 0, 100)).toBe(12.5);
    expect(typedValue('', 0, 100)).toBeNull();
    expect(typedValue('abc', 0, 100)).toBeNull();
    expect(typedValue('101', 0, 100)).toBeNull();
  });

  it('runs a reversed slider from the largest value to the smallest', () => {
    expect(reversedPosition(0.9, 0.1, 1)).toBeCloseTo(0.2);
    expect(reversedPosition(reversedPosition(0.3, 0.1, 1), 0.1, 1)).toBeCloseTo(0.3);
    expect(reversedPosition(5, 0.1, 1)).toBeCloseTo(0.1);
  });
});
