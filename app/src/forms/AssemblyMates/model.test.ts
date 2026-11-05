import { describe, expect, it } from 'vitest';

import type { MateRow } from '../../bridge/assembly';
import {
  conflictCount,
  kindsFor,
  mateRowText,
  mateValueField,
  mateValueText,
  offText,
  parseMateValue,
} from './model';

const row = (over: Partial<MateRow> = {}): MateRow => ({
  id: 1,
  kind: 'against',
  a: { sheet: 1, part: 1 },
  b: { sheet: 2, part: 1 },
  value: 0,
  aligned: false,
  suppressed: false,
  state: 'solved',
  distanceError: 0,
  angleError: 0,
  ...over,
});

describe('mates list', () => {
  it('reads each mate as its kind and its two pieces', () => {
    const name = (p: { sheet: number }) => `Panel ${p.sheet}`;
    expect(mateRowText(row(), name)).toBe('Against: Panel 1, Panel 2');
    expect(mateRowText(row({ kind: 'concentric' }), name)).toBe('Concentric: Panel 1, Panel 2');
  });

  it('gives each kind its value field', () => {
    expect(mateValueField('against')).toBe('offset');
    expect(mateValueField('flush')).toBe('offset');
    expect(mateValueField('distance')).toBe('distance');
    expect(mateValueField('angle')).toBe('angle');
    expect(mateValueField('concentric')).toBeNull();
    expect(mateValueField('joint')).toBeNull();
  });

  it('reads gaps in the job unit and angles in degrees from 0 to 180', () => {
    expect(parseMateValue('distance', '1', 'inch')).toBeCloseTo(25.4, 9);
    expect(parseMateValue('against', '-2', 'mm')).toBe(-2);
    expect(parseMateValue('angle', '90', 'inch')).toBe(90);
    expect(parseMateValue('angle', '200', 'mm')).toBeNull();
    expect(parseMateValue('flush', 'x', 'mm')).toBeNull();
    expect(mateValueText('angle', 45.00001, 'mm')).toBe('45');
    expect(mateValueText('distance', 25.4, 'inch')).toBe('1');
  });

  it('lets faces change kind among the flat kinds and keeps round axes concentric', () => {
    expect(kindsFor(row())).toEqual(['against', 'flush', 'distance', 'angle']);
    expect(kindsFor(row({ kind: 'concentric' }))).toEqual(['concentric']);
    expect(kindsFor(row({ kind: 'joint' }))).toEqual(['joint']);
    expect(mateRowText(row({ kind: 'joint' }), (p) => `Panel ${p.sheet}`)).toBe(
      'Joint: Panel 1, Panel 2',
    );
  });

  it('counts conflicts and says how far off one is', () => {
    const off = row({ state: 'conflict', distanceError: 1.5, angleError: 2 });
    expect(conflictCount([row(), off, row({ state: 'suppressed' })])).toBe(1);
    expect(offText(off, 'mm')).toBe('Off by 1.5 and 2°');
  });
});
