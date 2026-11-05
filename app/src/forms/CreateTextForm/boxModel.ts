// Pure state for Create Text's Text in a Box mode: the box the text fills
// (dragged in the 2D view, else the selected vectors, else the material),
// its margin and how spare room is used.

import type {
  AutoLayout,
  Bbox,
  HorizontalStretch,
  LayoutMargin,
  TextSpec,
  VerticalStretch,
} from '../../bridge/document';
import type { JobSetup, Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { jobBounds } from '../../views/View2D/camera';

export interface AutoLayoutState {
  margin: LayoutMargin;
  vertical: VerticalStretch;
  horizontal: HorizontalStretch;
}

export function initialAutoLayoutState(): AutoLayoutState {
  return { margin: 'normal', vertical: 'none', horizontal: 'none' };
}

/** The box the text fills: round the selected vectors, else the material. */
export function layoutBounds(selection: Bbox | null, job: JobSetup): Bbox {
  if (selection !== null && selection[2] > selection[0] && selection[3] > selection[1]) {
    return selection;
  }
  const bounds = jobBounds(job);
  return [bounds.minX, bounds.minY, bounds.maxX, bounds.maxY];
}

/** The text spec laid out into the box. The size only seeds the layout. */
export function autoLayoutSpec(spec: TextSpec, bounds: Bbox, state: AutoLayoutState): TextSpec {
  const layout: AutoLayout = { bounds, ...state };
  const next: TextSpec = {
    ...spec,
    sizeMm: spec.sizeMm > 0 ? spec.sizeMm : 10,
    autoLayout: layout,
  };
  delete next.wrapWidth;
  return next;
}

/** Form state for an auto layout text being edited. */
export function stateFromLayout(layout: AutoLayout): AutoLayoutState {
  return { margin: layout.margin, vertical: layout.vertical, horizontal: layout.horizontal };
}

/** The box being edited: its lower left corner in millimetres and its size
 * as typed in the Bounding Box Dimensions fields. */
export interface TextBoxState {
  x: number;
  y: number;
  width: string;
  height: string;
}

export function boxState(bounds: Bbox, units: Units): TextBoxState {
  return {
    x: bounds[0],
    y: bounds[1],
    width: lengthText(bounds[2] - bounds[0], units),
    height: lengthText(bounds[3] - bounds[1], units),
  };
}

export function convertBoxUnits(state: TextBoxState, from: Units, to: Units): TextBoxState {
  return convertLengthFields(state, ['width', 'height'], from, to);
}

export function boxErrors(
  state: TextBoxState,
  units: Units,
): Partial<Record<'width' | 'height', string>> {
  const errors: Partial<Record<'width' | 'height', string>> = {};
  const width = fieldError(lengthValue(state.width, units), { positive: true });
  const height = fieldError(lengthValue(state.height, units), { positive: true });
  if (width !== undefined) {
    errors.width = width;
  }
  if (height !== undefined) {
    errors.height = height;
  }
  return errors;
}

/** The box in millimetres, or null while a size is not valid. */
export function boxBounds(state: TextBoxState, units: Units): Bbox | null {
  const width = lengthValue(state.width, units);
  const height = lengthValue(state.height, units);
  if (width === null || height === null || width <= 0 || height <= 0) {
    return null;
  }
  return [state.x, state.y, state.x + width, state.y + height];
}

/** The box a drag from `a` to `b` draws, or null when it is too small to
 * be a box (a click). */
export function boxFromDrag(
  a: { x: number; y: number },
  b: { x: number; y: number },
  minimum: number,
): Bbox | null {
  const box: Bbox = [
    Math.min(a.x, b.x),
    Math.min(a.y, b.y),
    Math.max(a.x, b.x),
    Math.max(a.y, b.y),
  ];
  return box[2] - box[0] < minimum || box[3] - box[1] < minimum ? null : box;
}
