// Pure state for the Transform Component form: a relative move, a scale in
// percent and a turn about the component centre or a typed point.

import type { Path, Point } from '../../bridge/document';
import type { ComponentTransformParams } from '../../bridge/model';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

/** Component extent in job space, mm: minX, minY, maxX, maxY. */
export type Bbox = readonly [number, number, number, number];

export interface TransformFormState {
  dx: string;
  dy: string;
  /** Percent. */
  scaleX: string;
  scaleY: string;
  /** Scale Y follows Scale X while on. */
  linked: boolean;
  /** Degrees, counter clockwise. */
  angle: string;
  pivotAtCentre: boolean;
  pivotX: string;
  pivotY: string;
}

export type TransformField = Exclude<keyof TransformFormState, 'linked' | 'pivotAtCentre'>;
export type TransformErrors = Partial<Record<TransformField, string>>;

const LENGTH_FIELDS = ['dx', 'dy', 'pivotX', 'pivotY'] as const;

export function initialTransformState(units: Units): TransformFormState {
  return {
    dx: lengthText(0, units),
    dy: lengthText(0, units),
    scaleX: '100',
    scaleY: '100',
    linked: true,
    angle: '0',
    pivotAtCentre: true,
    pivotX: lengthText(0, units),
    pivotY: lengthText(0, units),
  };
}

export function convertTransformUnits(
  state: TransformFormState,
  from: Units,
  to: Units,
): TransformFormState {
  return convertLengthFields(state, LENGTH_FIELDS, from, to);
}

/** Sets one field; while linked, either scale field sets both. */
export function setTransformField(
  state: TransformFormState,
  field: TransformField,
  value: string,
): TransformFormState {
  if (state.linked && (field === 'scaleX' || field === 'scaleY')) {
    return { ...state, scaleX: value, scaleY: value };
  }
  return { ...state, [field]: value };
}

/** Turns the link on or off; turning it on copies Scale X to Scale Y. */
export function setLinked(state: TransformFormState, linked: boolean): TransformFormState {
  return linked ? { ...state, linked, scaleY: state.scaleX } : { ...state, linked };
}

/** Centre of a component's extent. */
export function bboxCentre(bbox: Bbox): Point {
  return { x: (bbox[0] + bbox[2]) / 2, y: (bbox[1] + bbox[3]) / 2 };
}

export function validateTransform(state: TransformFormState, units: Units): TransformErrors {
  const checks: Array<[TransformField, string | undefined]> = [
    ['dx', fieldError(lengthValue(state.dx, units))],
    ['dy', fieldError(lengthValue(state.dy, units))],
    ['scaleX', fieldError(numberValue(state.scaleX), { positive: true })],
    ['scaleY', fieldError(numberValue(state.scaleY), { positive: true })],
    ['angle', fieldError(numberValue(state.angle))],
  ];
  if (!state.pivotAtCentre) {
    checks.push(['pivotX', fieldError(lengthValue(state.pivotX, units))]);
    checks.push(['pivotY', fieldError(lengthValue(state.pivotY, units))]);
  }
  const errors: TransformErrors = {};
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** Kernel parameters, or null while the form is not valid. The centre is
 * left out for the component centre, which the kernel takes by default. */
export function transformParams(
  state: TransformFormState,
  units: Units,
): ComponentTransformParams | null {
  if (Object.keys(validateTransform(state, units)).length > 0) {
    return null;
  }
  const mm = (value: string) => lengthValue(value, units) ?? 0;
  const params: ComponentTransformParams = {
    offset: { x: mm(state.dx), y: mm(state.dy) },
    scaleX: (numberValue(state.scaleX) ?? 100) / 100,
    scaleY: (numberValue(state.scaleY) ?? 100) / 100,
    rotateDeg: numberValue(state.angle) ?? 0,
  };
  if (!state.pivotAtCentre) {
    params.centre = { x: mm(state.pivotX), y: mm(state.pivotY) };
  }
  return params;
}

/** Where the transform takes a point, as the kernel maps it. */
export function transformPoint(p: Point, params: ComponentTransformParams, centre: Point): Point {
  const radians = (params.rotateDeg * Math.PI) / 180;
  const sin = Math.sin(radians);
  const cos = Math.cos(radians);
  const x = (p.x - centre.x) * params.scaleX;
  const y = (p.y - centre.y) * params.scaleY;
  return {
    x: centre.x + x * cos - y * sin + params.offset.x,
    y: centre.y + x * sin + y * cos + params.offset.y,
  };
}

/** The component's box corners after the transform, counter clockwise
 * from the low corner. */
export function transformedCorners(bbox: Bbox, params: ComponentTransformParams): Point[] {
  const centre = params.centre ?? bboxCentre(bbox);
  const [minX, minY, maxX, maxY] = bbox;
  return [
    { x: minX, y: minY },
    { x: maxX, y: minY },
    { x: maxX, y: maxY },
    { x: minX, y: maxY },
  ].map((p) => transformPoint(p, params, centre));
}

/** Half the arm length of the pivot marker, mm. */
export const PIVOT_MARK = 2;

/** Preview paths: the transformed box as a closed path, and a small cross
 * at the pivot. Empty while the form is not valid. */
export function transformPreview(
  state: TransformFormState,
  units: Units,
  bbox: Bbox | undefined,
): Path[] {
  const params = bbox === undefined ? null : transformParams(state, units);
  if (bbox === undefined || params === null) {
    return [];
  }
  const [first, ...rest] = transformedCorners(bbox, params);
  if (first === undefined) {
    return [];
  }
  const box: Path = {
    subpaths: [{ start: first, segments: rest.map((to) => ({ kind: 'line', to })), closed: true }],
  };
  const c = params.centre ?? bboxCentre(bbox);
  const arm = (from: Point, to: Point) => ({
    start: from,
    segments: [{ kind: 'line' as const, to }],
    closed: false,
  });
  const pivot: Path = {
    subpaths: [
      arm({ x: c.x - PIVOT_MARK, y: c.y }, { x: c.x + PIVOT_MARK, y: c.y }),
      arm({ x: c.x, y: c.y - PIVOT_MARK }, { x: c.x, y: c.y + PIVOT_MARK }),
    ],
  };
  return [box, pivot];
}
