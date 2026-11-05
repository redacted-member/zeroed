// The profile angle handle of a round or angled shape in the 3D view: it
// stands out in front of the middle of the component's front edge, half way
// up the shape, and dragging it up steepens the edge angle and down flattens
// it; the shape is made again from its vectors at each step (one undo step
// for the drag).

import type { ComponentSummary, ShapeArgs } from '../../bridge/model';
import type { Units } from '../../bridge/types';
import {
  shapeArgs,
  shapeVectors,
  stateFromComponent,
  toolOf,
} from '../../forms/CreateShapeForm/model';
import type { Heights } from './heightHandles';
import type { Bbox, XY } from './transformHandles';

/** The profile angles a shape takes, degrees from flat at the edge. */
export const MIN_ANGLE = 1;
export const MAX_ANGLE = 90;

/** How far out in front of the box the handle stands, as a fraction of its
 * larger side. */
const REACH = 0.12;

/** The edge angle of a round or angled shape, or null for any other
 * component. */
export function profileAngleOf(component: ComponentSummary): number | null {
  const params = component.shape?.params;
  if (params === undefined || (params.profile !== 'round' && params.profile !== 'angular')) {
    return null;
  }
  return params.angle;
}

/** Where the handle stands in job XY. */
export function profilePointOf(bbox: Bbox): XY {
  const span = Math.max(bbox[2] - bbox[0], bbox[3] - bbox[1]);
  return { x: (bbox[0] + bbox[2]) / 2, y: bbox[1] - span * REACH };
}

/** Its height above the zero plane: half way up the shape. */
export function profileHeightOf(heights: Heights): number {
  return heights.base + heights.shape / 2;
}

export function clampAngle(angle: number): number {
  return Math.min(MAX_ANGLE, Math.max(MIN_ANGLE, angle));
}

/** The angle a drag of `dz` millimetres up the handle asks for: the whole
 * range over the box's larger side. */
export function dragAngle(start: number, dz: number, bbox: Bbox): number {
  const span = Math.max(bbox[2] - bbox[0], bbox[3] - bbox[1], 1);
  return Math.round(clampAngle(start + (dz * (MAX_ANGLE - MIN_ANGLE)) / span) * 10) / 10;
}

/** A typed angle, or null when it is out of range. */
export function typedAngle(value: number | undefined): number | null {
  if (value === undefined || !Number.isFinite(value) || value <= 0 || value > MAX_ANGLE) {
    return null;
  }
  return value;
}

/** The `model.createShape` arguments that make `component` again with the
 * edge angle `angle`, everything else as it was last applied. */
export function profileArgs(
  component: ComponentSummary,
  angle: number,
  units: Units,
): ShapeArgs | null {
  const params = component.shape?.params;
  if (params === undefined) {
    return null;
  }
  const state = { ...stateFromComponent(component, units), angle: String(angle) };
  const args = shapeArgs(state, toolOf(params.profile), units, shapeVectors([], component));
  return args === null ? null : { ...args, target: component.id };
}
