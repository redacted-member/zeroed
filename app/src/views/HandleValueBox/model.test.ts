import { describe, expect, it } from 'vitest';

import type { ValueField } from '../View3D/handleValues';
import { fieldText, readValues } from './model';

const FIELDS: ValueField[] = [
  { key: 'width', value: 25.4, unit: 'length' },
  { key: 'angle', value: 12.34567, unit: 'angle' },
];

describe('handle value box text', () => {
  it('shows lengths in the job unit and plain numbers', () => {
    expect(fieldText(FIELDS[0] as ValueField, 'inch')).toBe('1');
    expect(fieldText(FIELDS[1] as ValueField, 'mm')).toBe('12.346');
  });

  it('reads typed text back, untouched fields as shown', () => {
    expect(readValues(FIELDS, { width: '2' }, 'inch')).toEqual({ width: 50.8, angle: 12.346 });
    expect(readValues(FIELDS, { angle: 'abc' }, 'mm')).toBeNull();
  });
});
