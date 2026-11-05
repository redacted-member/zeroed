// Pure state for the Nest Parts form: spacing, orientation, the copies and
// filler mark Apply sets on the selected objects (the objects keep them, in
// the kernel) and the sheets to nest onto, each at its own size.

import type { EntitySummary, NestMark, NestParams, NestSheetUse } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

export interface NestFormState {
  toolDiameter: string;
  gap: string;
  border: string;
  /** Rotate Parts to find best fit, in steps of `rotationStep` degrees. */
  rotate: boolean;
  rotationStep: string;
  allowMirror: boolean;
  startCorner: NestParams['startCorner'];
  direction: NestParams['direction'];
  /** Let parts go inside the holes of parts already placed. */
  nestInHoles: boolean;
  /** Individual Part Properties: the Number of copies field. */
  partCopies: string;
  /** Individual Part Properties: the Filler tick. */
  partFiller: boolean;
  /** Custom Sheet Selection; null is the default, the active sheet with
   * new copies of it as needed. */
  sheetPlan: NestSheetUse[] | null;
  /** Move the originals; off leaves them and places copies. */
  removeOriginals: boolean;
  /** Carry the other side's objects with each part (double sided jobs). */
  twoSided: boolean;
  /** Nest boundary layer: its first closed vector bounds the nest instead
   * of the material; null nests on the material. */
  boundaryLayer: number | null;
}

/** A mark in the 2D view on an object with copies or a filler part. */
export interface NestMarker {
  id: number;
  /** The middle of the object's box, in job millimetres. */
  at: { x: number; y: number };
  /** The number of copies, `*` for a filler, or both. */
  text: string;
}

/** Most sheets a nest with No Limit may add. */
export const NEST_SHEET_LIMIT = 100;

export type NestField = 'toolDiameter' | 'gap' | 'border' | 'rotationStep';
export type NestErrors = Partial<Record<NestField, string>>;

const LENGTH_FIELDS: readonly NestField[] = ['toolDiameter', 'gap', 'border'];

export function initialNestState(units: Units): NestFormState {
  const inch = units === 'inch';
  return {
    toolDiameter: lengthText(inch ? 6.35 : 6, units),
    gap: lengthText(inch ? 1.27 : 1, units),
    border: lengthText(inch ? 12.7 : 10, units),
    rotate: true,
    rotationStep: '90',
    allowMirror: false,
    startCorner: 'bottomLeft',
    direction: 'x',
    nestInHoles: false,
    partCopies: '1',
    partFiller: false,
    sheetPlan: null,
    removeOriginals: true,
    boundaryLayer: null,
    twoSided: false,
  };
}

export function convertNestUnits(state: NestFormState, from: Units, to: Units): NestFormState {
  return convertLengthFields(state, LENGTH_FIELDS, from, to);
}

export function validateNest(state: NestFormState, units: Units): NestErrors {
  const errors: NestErrors = {};
  const set = (field: NestField, error: string | undefined) => {
    if (error !== undefined) {
      errors[field] = error;
    }
  };
  set('toolDiameter', fieldError(lengthValue(state.toolDiameter, units), { min: 0 }));
  set('gap', fieldError(lengthValue(state.gap, units), { min: 0 }));
  set('border', fieldError(lengthValue(state.border, units), { min: 0 }));
  if (state.rotate) {
    set('rotationStep', fieldError(numberValue(state.rotationStep), { min: 1, max: 360 }));
  }
  return errors;
}

/** The error of the Number of copies field: a whole number from 1 to 1000. */
export function partCopiesError(state: NestFormState): string | undefined {
  return fieldError(numberValue(state.partCopies), { min: 1, max: 1000, integer: true });
}

/** What Apply sets on the selected objects, or null while the Number of
 * copies does not read. */
export function partMark(state: NestFormState): NestMark | null {
  const copies = numberValue(state.partCopies);
  if (partCopiesError(state) !== undefined || copies === null) {
    return null;
  }
  return { copies, filler: state.partFiller };
}

/**
 * The Individual Part Properties of a new selection: the copies and filler
 * mark of its first object that has a mark, else 1 and not a filler.
 */
export function partPropertiesOf(
  entities: readonly EntitySummary[],
  ids: readonly number[],
): Pick<NestFormState, 'partCopies' | 'partFiller'> {
  const byId = new Map(entities.map((entity) => [entity.id, entity]));
  const mark = ids.map((id) => byId.get(id)?.nest).find((m) => m !== undefined);
  if (mark === undefined) {
    return { partCopies: '1', partFiller: false };
  }
  return { partCopies: String(mark.copies), partFiller: mark.filler };
}

/** The green marks of the 2D view: copies above one, and `*` on filler parts. */
export function nestMarkers(entities: readonly EntitySummary[]): NestMarker[] {
  const markers: NestMarker[] = [];
  for (const entity of entities) {
    const mark = entity.nest;
    if (mark === undefined || (mark.copies <= 1 && !mark.filler)) {
      continue;
    }
    const [minX, minY, maxX, maxY] = entity.bbox;
    const count = mark.copies > 1 ? String(mark.copies) : '';
    markers.push({
      id: entity.id,
      at: { x: (minX + maxX) / 2, y: (minY + maxY) / 2 },
      text: `${count}${mark.filler ? '*' : ''}`,
    });
  }
  return markers;
}

/** Kernel parameters, or null while a field is invalid. Each sheet nests at its own size. */
export function nestParams(state: NestFormState, units: Units): NestParams | null {
  if (Object.keys(validateNest(state, units)).length > 0) {
    return null;
  }
  return {
    toolDiameter: lengthValue(state.toolDiameter, units) ?? 0,
    gap: lengthValue(state.gap, units) ?? 0,
    border: lengthValue(state.border, units) ?? 0,
    rotationStepDeg: state.rotate ? (numberValue(state.rotationStep) ?? 90) : 0,
    allowMirror: state.allowMirror,
    sheetWidth: 0,
    sheetHeight: 0,
    startCorner: state.startCorner,
    direction: state.direction,
    maxSheets: NEST_SHEET_LIMIT,
    nestInHoles: state.nestInHoles,
  };
}

/** The options of ops.nest beyond the nesting parameters. */
export interface NestOptions {
  components: number[];
  removeOriginals: boolean;
  twoSided: boolean;
  boundaryLayer?: number;
  /** Custom Sheet Selection; left out for the active sheet and new copies of it. */
  sheetPlan?: NestSheetUse[];
}

export function nestOptions(
  state: NestFormState,
  pickedComponents: readonly number[],
  doubleSided: boolean,
): NestOptions {
  return {
    components: [...pickedComponents],
    removeOriginals: state.removeOriginals,
    twoSided: doubleSided && state.twoSided,
    ...(state.boundaryLayer === null ? {} : { boundaryLayer: state.boundaryLayer }),
    ...(state.sheetPlan === null ? {} : { sheetPlan: state.sheetPlan }),
  };
}
