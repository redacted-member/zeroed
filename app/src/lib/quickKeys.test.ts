import { describe, expect, it } from 'vitest';

import { applyQuickKey, parseQuickEntry, quickKeyAction, quickStepPoint } from './quickKeys';

const mm = (v: number) => v;

describe('quick key entry', () => {
  it('parses plain, paired and tagged values', () => {
    expect(parseQuickEntry('12.5')).toEqual({ values: [12.5], tags: {} });
    expect(parseQuickEntry('10,20')).toEqual({ values: [10, 20], tags: {} });
    expect(parseQuickEntry('5r40x')).toEqual({ values: [], tags: { r: 5, x: 40 } });
    expect(parseQuickEntry('6s25d')).toEqual({ values: [], tags: { s: 6, d: 25 } });
    expect(parseQuickEntry('-3, .5')).toEqual({ values: [-3, 0.5], tags: {} });
  });

  it('rejects text that is not an entry', () => {
    expect(parseQuickEntry('')).toBeNull();
    expect(parseQuickEntry('x')).toBeNull();
    expect(parseQuickEntry('5q')).toBeNull();
    expect(parseQuickEntry('1..2')).toBeNull();
  });

  it('starts only on a number so letter shortcuts keep working', () => {
    expect(quickKeyAction('', 'r')).toBe('ignore');
    expect(quickKeyAction('', 'Enter')).toBe('ignore');
    expect(quickKeyAction('', '5')).toBe('append');
    expect(quickKeyAction('5', 'r')).toBe('append');
    expect(quickKeyAction('5', 'Enter')).toBe('commit');
    expect(applyQuickKey('5r', 'Backspace')).toBe('5');
    expect(applyQuickKey('5', 'X')).toBe('5x');
  });

  it('steps from the last point by distance, offset, position and angle', () => {
    const from = { x: 10, y: 10 };
    const cursor = { x: 10, y: 50 };
    const step = (text: string) =>
      quickStepPoint(parseQuickEntry(text) ?? { values: [], tags: {} }, from, cursor, mm);
    expect(step('20')).toEqual({ x: 10, y: 30 });
    expect(step('5,-5')).toEqual({ x: 15, y: 5 });
    expect(step('100x')).toEqual({ x: 100, y: 50 });
    expect(step('1x2y')).toEqual({ x: 1, y: 2 });
    const polar = step('0a10l');
    expect(polar?.x).toBeCloseTo(20);
    expect(polar?.y).toBeCloseTo(10);
    expect(step('1,2,3')).toBeNull();
  });

  it('takes X,Y as the point itself for Draw Line', () => {
    const entry = parseQuickEntry('5,-5') ?? { values: [], tags: {} };
    expect(quickStepPoint(entry, { x: 10, y: 10 }, { x: 0, y: 0 }, mm, 'absolute')).toEqual({
      x: 5,
      y: -5,
    });
  });

  it('converts typed lengths through the job units', () => {
    const entry = parseQuickEntry('1,0');
    expect(entry).not.toBeNull();
    if (entry !== null) {
      expect(quickStepPoint(entry, { x: 0, y: 0 }, { x: 0, y: 0 }, (v) => v * 25.4)).toEqual({
        x: 25.4,
        y: 0,
      });
    }
  });
});
