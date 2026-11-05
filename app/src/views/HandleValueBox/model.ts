// Text of the handle value box: values shown in the job's unit, typed text
// read back to millimetres, percent or degrees.

import type { Units } from '../../bridge/types';
import { lengthText, lengthValue, numberValue } from '../../lib/formFields';
import type { TypedValues, ValueField } from '../View3D/handleValues';

/** A plain number with at most three decimals. */
function plain(value: number): string {
  return String(Math.round(value * 1000) / 1000);
}

export function fieldText(field: ValueField, units: Units): string {
  return field.unit === 'length' ? lengthText(field.value, units) : plain(field.value);
}

/** The typed values, or null when one of them is not a number. */
export function readValues(
  fields: readonly ValueField[],
  texts: Readonly<Record<string, string>>,
  units: Units,
): TypedValues | null {
  const values: TypedValues = {};
  for (const field of fields) {
    const text = texts[field.key] ?? fieldText(field, units);
    const value = field.unit === 'length' ? lengthValue(text, units) : numberValue(text);
    if (value === null) {
      return null;
    }
    values[field.key] = value;
  }
  return values;
}
