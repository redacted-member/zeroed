// Pure state for the Linear Array Copy form.

import type { ArrayCopyArgs, BlockTurn } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

export interface ArrayCopyFormState {
  rows: string;
  cols: string;
  mode: ArrayCopyArgs['mode'];
  gapX: string;
  gapY: string;
  /** Row / Column displacement, X and Y of each. */
  rowX: string;
  rowY: string;
  colX: string;
  colY: string;
  /** Block symmetry of the two by two block, first row first. */
  symmetry: BlockTurn[];
  group: boolean;
}

export type ArrayCopyField = 'rows' | 'cols' | 'gapX' | 'gapY' | 'rowX' | 'rowY' | 'colX' | 'colY';

/** The turns a symmetry button steps through. */
export const BLOCK_TURNS: readonly BlockTurn[] = ['none', 'mirrorX', 'mirrorY', 'rotate'];

/** The next turn of a symmetry button; the first place is the original. */
export function nextTurn(symmetry: readonly BlockTurn[], index: number): BlockTurn[] {
  const next = [...symmetry];
  if (index === 0) {
    return next;
  }
  const at = BLOCK_TURNS.indexOf(next[index] ?? 'none');
  next[index] = BLOCK_TURNS[(at + 1) % BLOCK_TURNS.length] ?? 'none';
  return next;
}
export type ArrayCopyErrors = Partial<Record<ArrayCopyField, string>>;

export function initialArrayCopyState(units: Units): ArrayCopyFormState {
  const gap = units === 'inch' ? 12.7 : 10;
  return {
    rows: '2',
    cols: '3',
    mode: 'gap',
    gapX: lengthText(gap, units),
    gapY: lengthText(gap, units),
    rowX: lengthText(0, units),
    rowY: lengthText(0, units),
    colX: lengthText(0, units),
    colY: lengthText(0, units),
    symmetry: ['none', 'none', 'none', 'none'],
    group: false,
  };
}

export function convertArrayCopyUnits(
  state: ArrayCopyFormState,
  from: Units,
  to: Units,
): ArrayCopyFormState {
  return convertLengthFields(state, ['gapX', 'gapY', 'rowX', 'rowY', 'colX', 'colY'], from, to);
}

export function validateArrayCopy(state: ArrayCopyFormState, units: Units): ArrayCopyErrors {
  const errors: ArrayCopyErrors = {};
  const count = { min: 1, max: 1000, integer: true };
  const rows = fieldError(numberValue(state.rows), count);
  const cols = fieldError(numberValue(state.cols), count);
  if (rows !== undefined) {
    errors.rows = rows;
  }
  if (cols !== undefined) {
    errors.cols = cols;
  }
  // Offsets are pitches between origins and must move the copy; gaps may be
  // zero (copies touching) or negative (overlapping).
  const pitch = state.mode === 'offset' ? { positive: true } : {};
  const gapX = fieldError(lengthValue(state.gapX, units), pitch);
  const gapY = fieldError(lengthValue(state.gapY, units), pitch);
  if (gapX !== undefined) {
    errors.gapX = gapX;
  }
  if (gapY !== undefined) {
    errors.gapY = gapY;
  }
  for (const field of ['rowX', 'rowY', 'colX', 'colY'] as const) {
    const error = fieldError(lengthValue(state[field], units));
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** Number of copies the grid makes, not counting the original. */
export function copyCount(state: ArrayCopyFormState): number {
  const rows = numberValue(state.rows) ?? 0;
  const cols = numberValue(state.cols) ?? 0;
  return Math.max(0, rows * cols - 1);
}

/** Kernel arguments, or null while a field is invalid or there is nothing to copy. */
export function arrayCopyArgs(state: ArrayCopyFormState, units: Units): ArrayCopyArgs | null {
  if (Object.keys(validateArrayCopy(state, units)).length > 0 || copyCount(state) === 0) {
    return null;
  }
  return {
    rows: numberValue(state.rows) ?? 1,
    cols: numberValue(state.cols) ?? 1,
    gapX: lengthValue(state.gapX, units) ?? 0,
    gapY: lengthValue(state.gapY, units) ?? 0,
    mode: state.mode,
    stagger: 'none',
    group: state.group,
    symmetry: [...state.symmetry],
    rowShift: [lengthValue(state.rowX, units) ?? 0, lengthValue(state.rowY, units) ?? 0],
    colShift: [lengthValue(state.colX, units) ?? 0, lengthValue(state.colY, units) ?? 0],
  };
}
