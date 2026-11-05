// Pure state for the Array Copy Toolpath form: the grid, its spacing and
// whether copies that share a tool are joined.

import type { ArrayCopySpec, ToolpathSummary } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

export interface ArrayCopyState {
  rows: string;
  columns: string;
  spacing: ArrayCopySpec['spacing'];
  x: string;
  y: string;
  minimize: boolean;
  name: string;
}

export type ArrayCopyField = 'rows' | 'columns' | 'x' | 'y';
export type ArrayCopyErrors = Partial<Record<ArrayCopyField | 'grid', string>>;

export function initialArrayCopyState(units: Units): ArrayCopyState {
  const gap = lengthText(units === 'inch' ? 12.7 : 10, units);
  return { rows: '2', columns: '2', spacing: 'gap', x: gap, y: gap, minimize: true, name: '' };
}

export function convertArrayCopyUnits(
  state: ArrayCopyState,
  from: Units,
  to: Units,
): ArrayCopyState {
  return convertLengthFields(state, ['x', 'y'], from, to);
}

/** The toolpaths the array copies: the visible ones, in list order. */
export function arraySources(toolpaths: readonly ToolpathSummary[]): ToolpathSummary[] {
  return toolpaths.filter((t) => t.visible);
}

/** Minimize tool changes only means something when the sources use more than one tool. */
export function mixesTools(sources: readonly ToolpathSummary[]): boolean {
  return new Set(sources.map((t) => t.toolName)).size > 1;
}

export function validateArrayCopy(
  state: ArrayCopyState,
  units: Units,
  gridError: string,
): ArrayCopyErrors {
  const errors: ArrayCopyErrors = {};
  const count = { min: 1, max: 1000, integer: true };
  const rows = numberValue(state.rows);
  const columns = numberValue(state.columns);
  const checks: Array<[ArrayCopyField, string | undefined]> = [
    ['rows', fieldError(rows, count)],
    ['columns', fieldError(columns, count)],
    ['x', fieldError(lengthValue(state.x, units))],
    ['y', fieldError(lengthValue(state.y, units))],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  if (
    errors.rows === undefined &&
    errors.columns === undefined &&
    (rows ?? 0) * (columns ?? 0) < 2
  ) {
    errors.grid = gridError;
  }
  return errors;
}

/** The kernel spec, or null while a field is invalid. */
export function arrayCopySpec(
  state: ArrayCopyState,
  units: Units,
  sources: readonly ToolpathSummary[],
): ArrayCopySpec | null {
  if (Object.keys(validateArrayCopy(state, units, '')).length > 0) {
    return null;
  }
  return {
    rows: numberValue(state.rows) ?? 1,
    columns: numberValue(state.columns) ?? 1,
    spacing: state.spacing,
    x: lengthValue(state.x, units) ?? 0,
    y: lengthValue(state.y, units) ?? 0,
    minimizeToolChanges: mixesTools(sources) && state.minimize,
  };
}
