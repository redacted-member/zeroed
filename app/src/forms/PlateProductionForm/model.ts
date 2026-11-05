// Pure state for Plate Production: margins and spacing on the material, the
// data file and how each template variable is filled (a data column or a
// counter).

import type { PlateField, PlateSeparator, PlatesProduceArgs } from '../../bridge/api';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

export type FieldChoice =
  | { kind: 'column'; column: number }
  | { kind: 'counter'; start: string; step: string; digits: string };

export interface PlateState {
  left: string;
  right: string;
  top: string;
  bottom: string;
  /** One margin value all round. */
  equalMargins: boolean;
  spacingX: string;
  spacingY: string;
  path: string | null;
  separator: PlateSeparator;
  header: boolean;
  /** Plates to make without a data file. */
  count: string;
  ownSheets: boolean;
  /** Mode: plates that do not fit go on new sheets or new layers. */
  mode: 'sheets' | 'layers';
  /** Calculate the template's toolpaths for the plates (when it has any). */
  toolpaths: boolean;
  fields: Record<string, FieldChoice>;
}

export type PlateLengthField = 'left' | 'right' | 'top' | 'bottom' | 'spacingX' | 'spacingY';
export type PlateErrorField = PlateLengthField | 'count' | `field:${string}`;
export type PlateErrors = Partial<Record<PlateErrorField, string>>;

const LENGTHS: readonly PlateLengthField[] = [
  'left',
  'right',
  'top',
  'bottom',
  'spacingX',
  'spacingY',
];

export function initialPlateState(units: Units): PlateState {
  const inch = units === 'inch';
  const margin = lengthText(inch ? 12.7 : 10, units);
  const gap = lengthText(inch ? 6.35 : 5, units);
  return {
    left: margin,
    right: margin,
    top: margin,
    bottom: margin,
    equalMargins: true,
    spacingX: gap,
    spacingY: gap,
    path: null,
    separator: 'comma',
    header: true,
    count: '1',
    ownSheets: false,
    mode: 'sheets',
    toolpaths: true,
    fields: {},
  };
}

export function convertPlateUnits(state: PlateState, from: Units, to: Units): PlateState {
  return convertLengthFields(state, LENGTHS, from, to);
}

/** Sets one margin; with equal margins the others follow. */
export function setMargin(
  state: PlateState,
  field: 'left' | 'right' | 'top' | 'bottom',
  value: string,
): PlateState {
  return state.equalMargins
    ? { ...state, left: value, right: value, top: value, bottom: value }
    : { ...state, [field]: value };
}

function defaultChoice(variable: string, index: number, columns: readonly string[]): FieldChoice {
  const named = columns.findIndex((c) => c.trim().toLowerCase() === variable.toLowerCase());
  if (named >= 0) {
    return { kind: 'column', column: named };
  }
  if (columns.length > 0) {
    return { kind: 'column', column: Math.min(index, columns.length - 1) };
  }
  return { kind: 'counter', start: '1', step: '1', digits: '0' };
}

/**
 * Gives every template variable a source: kept when it still works, else
 * the column of the same name, the column in the same place, or a counter
 * without a data file.
 */
export function withVariables(
  state: PlateState,
  variables: readonly string[],
  columns: readonly string[],
): PlateState {
  const fields: Record<string, FieldChoice> = {};
  variables.forEach((variable, index) => {
    const current = state.fields[variable];
    const usable =
      current !== undefined &&
      (current.kind === 'counter' ||
        (current.kind === 'column' && current.column < columns.length));
    fields[variable] = usable ? current : defaultChoice(variable, index, columns);
  });
  return { ...state, fields };
}

export interface GridSize {
  columns: number;
  rows: number;
}

/** Plates of `plate` size that fit in `space` with `gap` between them. */
export function plateGrid(
  space: { width: number; height: number },
  plate: { width: number; height: number },
  gap: { x: number; y: number },
): GridSize {
  const fit = (room: number, size: number, spacing: number) =>
    size <= 0 || room + 1e-9 < size ? 0 : Math.floor((room - size) / (size + spacing) + 1e-9) + 1;
  return {
    columns: fit(space.width, plate.width, gap.x),
    rows: fit(space.height, plate.height, gap.y),
  };
}

/** Plates per sheet for the form's margins and spacing, or null while one is invalid. */
export function platesPerSheet(
  state: PlateState,
  units: Units,
  material: { width: number; height: number },
  plate: { width: number; height: number } | null,
): number | null {
  const value = (field: PlateLengthField) => lengthValue(state[field], units);
  const left = value('left');
  const right = value('right');
  const top = value('top');
  const bottom = value('bottom');
  const x = value('spacingX');
  const y = value('spacingY');
  if (
    plate === null ||
    left === null ||
    right === null ||
    top === null ||
    bottom === null ||
    x === null ||
    y === null
  ) {
    return null;
  }
  if (state.ownSheets) {
    return 1;
  }
  const grid = plateGrid(
    { width: material.width - left - right, height: material.height - top - bottom },
    plate,
    { x, y },
  );
  return grid.columns * grid.rows;
}

export function validatePlates(state: PlateState, units: Units): PlateErrors {
  const errors: PlateErrors = {};
  for (const field of LENGTHS) {
    const error = fieldError(lengthValue(state[field], units), { min: 0 });
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  if (state.path === null) {
    const error = fieldError(numberValue(state.count), { min: 1, max: 5000, integer: true });
    if (error !== undefined) {
      errors.count = error;
    }
  }
  for (const [name, choice] of Object.entries(state.fields)) {
    if (choice.kind !== 'counter') {
      continue;
    }
    const bad =
      fieldError(numberValue(choice.start), { integer: true }) ??
      fieldError(numberValue(choice.step), { integer: true }) ??
      fieldError(numberValue(choice.digits), { min: 0, max: 12, integer: true });
    if (bad !== undefined) {
      errors[`field:${name}`] = bad;
    }
  }
  return errors;
}

/** Kernel arguments for plates.produce, or null while something is invalid. */
export function plateArgs(
  state: PlateState,
  units: Units,
  ids: readonly number[],
  templateToolpaths = 0,
): PlatesProduceArgs | null {
  if (ids.length === 0 || Object.keys(validatePlates(state, units)).length > 0) {
    return null;
  }
  const length = (field: PlateLengthField) => lengthValue(state[field], units) ?? 0;
  const fields: Record<string, PlateField> = {};
  for (const [name, choice] of Object.entries(state.fields)) {
    fields[name] =
      choice.kind === 'column'
        ? { kind: 'column', column: choice.column }
        : {
            kind: 'counter',
            start: numberValue(choice.start) ?? 1,
            step: numberValue(choice.step) ?? 1,
            digits: numberValue(choice.digits) ?? 0,
          };
  }
  const args: PlatesProduceArgs = {
    ids: [...ids],
    separator: state.separator,
    header: state.header,
    fields,
    margins: {
      left: length('left'),
      right: length('right'),
      top: length('top'),
      bottom: length('bottom'),
    },
    spacing: { x: length('spacingX'), y: length('spacingY') },
    ownSheets: state.ownSheets,
    mode: state.mode,
    toolpaths: state.toolpaths && templateToolpaths > 0,
  };
  if (state.path !== null) {
    args.path = state.path;
  } else {
    args.count = numberValue(state.count) ?? 1;
  }
  return args;
}
