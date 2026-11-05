// Pure state for Set Selection Size: width and height, linked or free. A rotated
// selection is sized along its own rotated frame, so the box passed in is
// the frame extent.

import type { Bbox, Matrix, SelectionFrame } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import type { Anchor } from '../../lib/shapes';
import { scaleAbout } from '../../lib/geometry';
import {
  bboxAnchor,
  bboxHeight,
  bboxWidth,
  setFrameSizeMatrix,
  type Placed,
  type PlacedMatrix,
} from '../../lib/transformMath';

export interface SetSizeFormState {
  /** Scale each selected item about its own anchor instead of the selection as one. */
  individually: boolean;
  anchor: Anchor;
  width: string;
  height: string;
  link: boolean;
  /** Auto Scale Z: selected components' heights scale with the size. */
  autoScaleZ: boolean;
}

export type SetSizeField = 'width' | 'height';
export type SetSizeErrors = Partial<Record<SetSizeField, string>>;

export function initialSetSizeState(units: Units): SetSizeFormState {
  return {
    individually: false,
    anchor: 'centre',
    width: lengthText(0, units),
    height: lengthText(0, units),
    link: true,
    autoScaleZ: false,
  };
}

export function convertSetSizeUnits(
  state: SetSizeFormState,
  from: Units,
  to: Units,
): SetSizeFormState {
  return convertLengthFields(state, ['width', 'height'], from, to);
}

/** Shows the selection's current size. */
export function sizeStateFromSelection(
  state: SetSizeFormState,
  units: Units,
  bbox: Bbox | null,
): SetSizeFormState {
  if (bbox === null) {
    return state;
  }
  return {
    ...state,
    width: lengthText(bboxWidth(bbox), units),
    height: lengthText(bboxHeight(bbox), units),
  };
}

/** Changes one field; with link on, the other follows in proportion. */
export function setSizeField(
  state: SetSizeFormState,
  units: Units,
  bbox: Bbox | null,
  field: SetSizeField,
  value: string,
): SetSizeFormState {
  const next = { ...state, [field]: value };
  if (!state.link || bbox === null) {
    return next;
  }
  const typed = lengthValue(value, units);
  if (typed === null) {
    return next;
  }
  const width = bboxWidth(bbox);
  const height = bboxHeight(bbox);
  if (width <= 0 || height <= 0) {
    return next;
  }
  const ratio = height / width;
  return field === 'width'
    ? { ...next, height: lengthText(typed * ratio, units) }
    : { ...next, width: lengthText(typed / ratio, units) };
}

export function validateSetSize(state: SetSizeFormState, units: Units): SetSizeErrors {
  const errors: SetSizeErrors = {};
  for (const field of ['width', 'height'] as const) {
    const value = lengthValue(state[field], units);
    const error = fieldError(value, { positive: true });
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

export function setSizeMatrixFromState(
  state: SetSizeFormState,
  units: Units,
  frame: SelectionFrame,
): Matrix | null {
  if (Object.keys(validateSetSize(state, units)).length > 0) {
    return null;
  }
  const width = lengthValue(state.width, units) ?? 0;
  const height = lengthValue(state.height, units) ?? 0;
  return setFrameSizeMatrix(frame, state.anchor, width, height);
}

/**
 * Scale items individually: the factors the fields give for the selection
 * as a whole, applied to each item about its own anchor.
 */
export function individualMatrices(
  state: SetSizeFormState,
  units: Units,
  selection: Bbox,
  items: readonly Placed[],
): PlacedMatrix[] | null {
  if (Object.keys(validateSetSize(state, units)).length > 0) {
    return null;
  }
  const width = bboxWidth(selection);
  const height = bboxHeight(selection);
  const sx = width > 0 ? (lengthValue(state.width, units) ?? width) / width : 1;
  const sy = height > 0 ? (lengthValue(state.height, units) ?? height) / height : 1;
  return items.map((item) => {
    const at = bboxAnchor(item.bbox, state.anchor);
    return { id: item.id, matrix: scaleAbout(sx, sy, at.x, at.y) };
  });
}

/**
 * The picture beside the Mode choice: the
 * selection scaled as a whole (one box round every shape) or each item
 * in its own box.
 */
export function setSizeModeIcon(individually: boolean): string {
  return individually ? 'sizeIndividually' : 'sizeAsOne';
}
