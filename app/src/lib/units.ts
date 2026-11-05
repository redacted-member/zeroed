// Length conversion and formatting. The kernel stores millimetres, the UI
// shows whatever unit the job is set to.

import type { Units } from '../bridge/types';
import { currentVariables, evaluate } from './expression';

/** Millimetres in one inch. */
export const MM_PER_INCH = 25.4;

/** Decimal places shown for a unit. */
export function decimalsFor(units: Units): number {
  return units === 'inch' ? 4 : 3;
}

/** Converts a stored millimetre value into the display unit. */
export function toDisplay(millimetres: number, units: Units): number {
  return units === 'inch' ? millimetres / MM_PER_INCH : millimetres;
}

/** Converts a value the user typed into millimetres. */
export function toMillimetres(value: number, units: Units): number {
  return units === 'inch' ? value * MM_PER_INCH : value;
}

/** Rounds away conversion noise so that a unit round trip is stable. */
export function roundForUnits(value: number, units: Units): number {
  const factor = 10 ** decimalsFor(units);
  return Math.round(value * factor) / factor;
}

/** Formats a stored millimetre value for a text field in the display unit. */
export function formatLength(millimetres: number, units: Units): string {
  return formatDisplay(toDisplay(millimetres, units), units);
}

/** Formats a value that is already in the display unit, trimming zeroes. */
export function formatDisplay(value: number, units: Units): string {
  const fixed = value.toFixed(decimalsFor(units));
  const trimmed = fixed.includes('.') ? fixed.replace(/\.?0+$/, '') : fixed;
  // A tiny negative rounds to "-0", which reads as a sign error.
  return trimmed === '-0' ? '0' : trimmed;
}

/**
 * Parses a typed length or number: arithmetic and document variables such
 * as `{width} / 2` work too. Returns null when the text is not a number.
 */
export function parseLength(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === '') {
    return null;
  }
  return evaluate(trimmed, currentVariables());
}

/** Short unit name for the status bar and field suffixes. */
export function unitSuffix(units: Units): string {
  return units === 'inch' ? 'inch' : 'mm';
}
