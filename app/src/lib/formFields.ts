// Helpers for form models that keep lengths as text in the display unit and
// hand millimetres to the kernel.

import type { Units } from '../bridge/types';
import { strings } from './strings';
import { formatLength, parseLength, toMillimetres } from './units';

/** Text for a millimetre value in the display unit. */
export function lengthText(millimetres: number, units: Units): string {
  return formatLength(millimetres, units);
}

/** Millimetres for a typed length, or null when it is not a number. */
export function lengthValue(text: string, units: Units): number | null {
  const typed = parseLength(text);
  return typed === null ? null : toMillimetres(typed, units);
}

/** Plain number for a unitless field (angles, counts, percentages). */
export function numberValue(text: string): number | null {
  return parseLength(text);
}

export interface FieldRule {
  positive?: boolean;
  min?: number;
  max?: number;
  integer?: boolean;
}

/** Validation message for one numeric field, or undefined when it is fine. */
export function fieldError(value: number | null, rule: FieldRule = {}): string | undefined {
  if (value === null) {
    return strings.form.errorNumber;
  }
  if (rule.positive === true && value <= 0) {
    return strings.form.errorPositive;
  }
  if (rule.min !== undefined && value < rule.min) {
    return strings.form.errorMin(rule.min);
  }
  if (rule.max !== undefined && value > rule.max) {
    return strings.form.errorMax(rule.max);
  }
  if (rule.integer === true && !Number.isInteger(value)) {
    return strings.form.errorNumber;
  }
  return undefined;
}

/** Keys of T whose values are strings. */
export type StringKeys<T> = { [K in keyof T]: T[K] extends string ? K : never }[keyof T];

/** Converts every length field when the display unit changes. */
export function convertLengthFields<T extends object>(
  state: T,
  fields: ReadonlyArray<StringKeys<T>>,
  from: Units,
  to: Units,
): T {
  if (from === to) {
    return state;
  }
  const next: Record<string, unknown> = { ...(state as Record<string, unknown>) };
  for (const field of fields) {
    const current = next[field as string];
    if (typeof current !== 'string') {
      continue;
    }
    const millimetres = lengthValue(current, from);
    if (millimetres !== null) {
      next[field as string] = lengthText(millimetres, to);
    }
  }
  return next as T;
}
