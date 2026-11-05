import { describe, expect, it } from 'vitest';

import {
  MM_PER_INCH,
  decimalsFor,
  formatDisplay,
  formatLength,
  parseLength,
  roundForUnits,
  toDisplay,
  toMillimetres,
} from './units';

describe('units', () => {
  it('converts millimetres to the display unit', () => {
    expect(toDisplay(25.4, 'inch')).toBeCloseTo(1, 10);
    expect(toDisplay(300, 'mm')).toBe(300);
  });

  it('converts typed values back to millimetres', () => {
    expect(toMillimetres(1, 'inch')).toBeCloseTo(MM_PER_INCH, 10);
    expect(toMillimetres(18, 'mm')).toBe(18);
  });

  it('shows more decimals for inches than for millimetres', () => {
    expect(decimalsFor('inch')).toBe(4);
    expect(decimalsFor('mm')).toBe(3);
  });

  it('formats without trailing zeroes', () => {
    expect(formatLength(300, 'mm')).toBe('300');
    expect(formatLength(18.5, 'mm')).toBe('18.5');
    expect(formatLength(25.4, 'inch')).toBe('1');
  });

  it('keeps a round trip through inches stable', () => {
    const millimetres = 609.6;
    const asInches = roundForUnits(toDisplay(millimetres, 'inch'), 'inch');
    expect(formatDisplay(asInches, 'inch')).toBe('24');
    expect(formatDisplay(-1e-12, 'mm')).toBe('0');
    expect(toMillimetres(asInches, 'inch')).toBeCloseTo(millimetres, 6);
  });

  it('rejects text that is not a number', () => {
    expect(parseLength('')).toBeNull();
    expect(parseLength('wide')).toBeNull();
    expect(parseLength('12.5')).toBe(12.5);
    expect(parseLength(' -3 ')).toBe(-3);
  });
});
