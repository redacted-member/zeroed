// Pure state for the Vector Unwrapper: the centre of rotation the contour
// is unrolled about, Refine unwrapped vectors, and whether it can run.

import type { Bbox, Point } from '../../bridge/document';
import type { JobSetup, Units } from '../../bridge/types';
import { convertLengthFields, lengthText, lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { isRotary } from '../../lib/rotary';

/** Use center of contour, of workspace, or a custom point. */
export type CentreChoice = 'contour' | 'workspace' | 'custom';

export interface VectorUnwrapperState {
  centre: CentreChoice;
  /** The custom point, in the display unit. */
  x: string;
  y: string;
  refine: boolean;
}

export function initialUnwrapper(units: Units, contour: Bbox | null): VectorUnwrapperState {
  const middle = contour === null ? { x: 0, y: 0 } : boxMiddle(contour);
  return {
    centre: 'contour',
    x: lengthText(middle.x, units),
    y: lengthText(middle.y, units),
    refine: true,
  };
}

export function convertUnwrapper(
  state: VectorUnwrapperState,
  from: Units,
  to: Units,
): VectorUnwrapperState {
  return convertLengthFields(state, ['x', 'y'], from, to);
}

export function boxMiddle(box: Bbox): Point {
  return { x: (box[0] + box[2]) / 2, y: (box[1] + box[3]) / 2 };
}

/** The centre of rotation in millimetres, or null while it cannot be told
 * (no contour, or a custom field that does not read). */
export function centrePoint(
  state: VectorUnwrapperState,
  contour: Bbox | null,
  workspace: Bbox,
  units: Units,
): Point | null {
  if (state.centre === 'workspace') {
    return boxMiddle(workspace);
  }
  if (state.centre === 'contour') {
    return contour === null ? null : boxMiddle(contour);
  }
  const x = lengthValue(state.x, units);
  const y = lengthValue(state.y, units);
  return x === null || y === null ? null : { x, y };
}

/** The custom point moved to `p` (a drag of its handle). */
export function customAt(
  state: VectorUnwrapperState,
  p: Point,
  units: Units,
): VectorUnwrapperState {
  return { ...state, centre: 'custom', x: lengthText(p.x, units), y: lengthText(p.y, units) };
}

/** Why the tool cannot run now, or null when it can: a rotary job and one
 * closed contour selected. */
export function unwrapBlocker(
  job: JobSetup,
  selectedCount: number,
  closed: boolean,
): string | null {
  if (!isRotary(job)) {
    return strings.vectorUnwrapper.needsRotary;
  }
  if (selectedCount !== 1 || !closed) {
    return strings.vectorUnwrapper.needsSelection;
  }
  return null;
}
