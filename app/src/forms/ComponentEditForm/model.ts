// Pure state for the Edit Component form: one set of fields per edit, of
// which only the chosen edit's fields are checked and sent.

import type { ComponentEdit, ComponentEditOp } from '../../bridge/model';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import { strings } from '../../lib/formStrings';

const text = strings.model.edit;

/** Kernel limit on smoothing passes in one edit. */
export const MAX_SMOOTH_ITERATIONS = 100;

export const EDIT_OPS: readonly ComponentEditOp[] = [
  'smooth',
  'draft',
  'tilt',
  'fade',
  'mirror',
  'flip',
  'scale',
  'emboss',
];

/** Edits that have settings worth a form; the rest apply at once. */
export function editHasSettings(op: ComponentEditOp): boolean {
  return op !== 'flip';
}

export interface EditFormState {
  strength: string;
  iterations: string;
  draftAngle: string;
  tiltAngle: string;
  tiltDirection: string;
  pivotAtCentre: boolean;
  pivotX: string;
  pivotY: string;
  fadeDirection: string;
  /** Percent of the component's extent. */
  fadeStart: string;
  fadeEnd: string;
  mirrorHorizontal: boolean;
  scale: string;
  baseOffset: string;
  maxHeight: string;
}

export type EditField = Exclude<keyof EditFormState, 'pivotAtCentre' | 'mirrorHorizontal'>;
export type EditErrors = Partial<Record<EditField, string>>;

/** Component extent in job space, mm: minX, minY, maxX, maxY. */
export type Bbox = readonly [number, number, number, number];

const LENGTH_FIELDS = ['strength', 'pivotX', 'pivotY', 'baseOffset', 'maxHeight'] as const;

export function initialEditState(units: Units): EditFormState {
  const inch = units === 'inch';
  return {
    strength: lengthText(inch ? 1.27 : 1, units),
    iterations: '1',
    draftAngle: '10',
    tiltAngle: '5',
    tiltDirection: '0',
    pivotAtCentre: true,
    pivotX: lengthText(0, units),
    pivotY: lengthText(0, units),
    fadeDirection: '0',
    fadeStart: '0',
    fadeEnd: '100',
    mirrorHorizontal: true,
    scale: '1',
    baseOffset: lengthText(0, units),
    maxHeight: lengthText(inch ? 2.54 : 2, units),
  };
}

export function convertEditUnits(state: EditFormState, from: Units, to: Units): EditFormState {
  return convertLengthFields(state, LENGTH_FIELDS, from, to);
}

/** Centre of a component's extent. */
export function bboxCentre(bbox: Bbox): { x: number; y: number } {
  return { x: (bbox[0] + bbox[2]) / 2, y: (bbox[1] + bbox[3]) / 2 };
}

function checks(
  op: ComponentEditOp,
  state: EditFormState,
  units: Units,
  bbox: Bbox | undefined,
): Array<[EditField, string | undefined]> {
  switch (op) {
    case 'smooth':
      return [
        ['strength', fieldError(lengthValue(state.strength, units), { positive: true })],
        [
          'iterations',
          fieldError(numberValue(state.iterations), {
            min: 1,
            max: MAX_SMOOTH_ITERATIONS,
            integer: true,
          }),
        ],
      ];
    case 'draft':
      return [['draftAngle', openRange(numberValue(state.draftAngle), 0, 90)]];
    case 'tilt': {
      const list: Array<[EditField, string | undefined]> = [
        ['tiltAngle', openRange(numberValue(state.tiltAngle), -90, 90)],
        ['tiltDirection', fieldError(numberValue(state.tiltDirection))],
      ];
      if (state.pivotAtCentre) {
        list.push(['pivotX', bbox === undefined ? text.noExtent : undefined]);
      } else {
        list.push(['pivotX', fieldError(lengthValue(state.pivotX, units))]);
        list.push(['pivotY', fieldError(lengthValue(state.pivotY, units))]);
      }
      return list;
    }
    case 'fade': {
      const start = numberValue(state.fadeStart);
      const end = numberValue(state.fadeEnd);
      const endError = fieldError(end, { min: 0, max: 100 });
      return [
        ['fadeDirection', fieldError(numberValue(state.fadeDirection))],
        ['fadeStart', fieldError(start, { min: 0, max: 100 })],
        [
          'fadeEnd',
          endError ?? (start !== null && end !== null && end <= start ? text.fadeOrder : undefined),
        ],
      ];
    }
    case 'scale':
      return [
        ['scale', fieldError(numberValue(state.scale))],
        ['baseOffset', fieldError(lengthValue(state.baseOffset, units))],
      ];
    case 'emboss':
      return [['maxHeight', fieldError(lengthValue(state.maxHeight, units), { positive: true })]];
    case 'mirror':
    case 'flip':
      return [];
  }
}

/** An error unless the value lies strictly between the bounds. */
function openRange(value: number | null, low: number, high: number): string | undefined {
  if (value === null) {
    return strings.form.errorNumber;
  }
  if (value <= low) {
    return strings.form.errorMin(low);
  }
  if (value >= high) {
    return strings.form.errorMax(high);
  }
  return undefined;
}

export function validateEdit(
  op: ComponentEditOp,
  state: EditFormState,
  units: Units,
  bbox: Bbox | undefined,
): EditErrors {
  const errors: EditErrors = {};
  for (const [field, error] of checks(op, state, units, bbox)) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** The edit to send, or null while the chosen edit's fields are not valid. */
export function editArgs(
  op: ComponentEditOp,
  state: EditFormState,
  units: Units,
  bbox: Bbox | undefined,
): ComponentEdit | null {
  if (Object.keys(validateEdit(op, state, units, bbox)).length > 0) {
    return null;
  }
  const num = (value: string) => numberValue(value) ?? 0;
  const mm = (value: string) => lengthValue(value, units) ?? 0;
  switch (op) {
    case 'smooth':
      return {
        op,
        params: { strength: mm(state.strength), iterations: num(state.iterations) },
      };
    case 'draft':
      return { op, params: { angleDeg: num(state.draftAngle) } };
    case 'tilt':
      return {
        op,
        params: {
          angleDeg: num(state.tiltAngle),
          directionDeg: num(state.tiltDirection),
          pivot:
            state.pivotAtCentre && bbox !== undefined
              ? bboxCentre(bbox)
              : { x: mm(state.pivotX), y: mm(state.pivotY) },
        },
      };
    case 'fade':
      return {
        op,
        params: {
          directionDeg: num(state.fadeDirection),
          startFraction: num(state.fadeStart) / 100,
          endFraction: num(state.fadeEnd) / 100,
        },
      };
    case 'mirror':
      return { op, params: { horizontal: state.mirrorHorizontal } };
    case 'flip':
      return { op };
    case 'scale':
      return { op, params: { scale: num(state.scale), baseOffset: mm(state.baseOffset) } };
    case 'emboss':
      return { op, params: { maxHeight: mm(state.maxHeight) } };
  }
}
