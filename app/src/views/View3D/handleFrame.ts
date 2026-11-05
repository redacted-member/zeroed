// The component showing transform handles in the 3D view, and where its
// handles land on the canvas. The projection comes from the renderer.

import type { ComponentSummary, ComponentTransformParams } from '../../bridge/model';
import { HEIGHT_KINDS, heightOf, heightsOf, type Heights } from './heightHandles';
import type { AnyHandle } from './handleValues';
import { profileAngleOf, profileHeightOf, profilePointOf } from './profileHandle';
import {
  applyParams,
  cornerOf,
  CORNERS,
  edgeOf,
  HANDLES,
  handlePoint,
  type Bbox,
  type XY,
} from './transformHandles';

export const NO_TRANSFORM: ComponentTransformParams = {
  offset: { x: 0, y: 0 },
  scaleX: 1,
  scaleY: 1,
  rotateDeg: 0,
};

/** The component with handles and the drag on them so far. */
export interface TransformFrame {
  id: number;
  bbox: Bbox;
  /** Scene height of the model's zero plane. */
  zeroY: number;
  /** Scene height the box handles lie at: the component's top. */
  planeY: number;
  params: ComponentTransformParams;
  /** The rotation centre in job XY, null for the middle of the box. */
  pivot: XY | null;
  /** The middle handle moves the rotation centre. */
  centreMode: boolean;
  /** The heights as the drag leaves them so far. */
  heights: Heights;
  /** A round or angled shape's edge angle as the drag leaves it; null for
   * any other component. */
  profile: number | null;
}

/** A frame for `component` as it stands, keeping the rotation centre and
 * mode of `previous` when it is the same component. */
export function frameFor(
  component: ComponentSummary,
  zeroY: number,
  previous: TransformFrame | null,
): TransformFrame | null {
  if (component.bbox === undefined) {
    return null;
  }
  const same = previous !== null && previous.id === component.id;
  return {
    id: component.id,
    bbox: component.bbox,
    zeroY,
    planeY: zeroY + Math.max(component.maxHeight ?? 0, 0),
    params: NO_TRANSFORM,
    pivot: same ? previous.pivot : null,
    centreMode: same ? previous.centreMode : false,
    heights: heightsOf(component),
    profile: profileAngleOf(component),
  };
}

/** Where the rotation centre goes after a finished drag: it rides along
 * with a move or a scale, and stays put for a turn about it. */
export function pivotAfter(frame: TransformFrame): XY | null {
  if (frame.pivot === null) {
    return null;
  }
  return applyParams(frame.bbox, frame.params, frame.pivot);
}

function spanOf(bbox: Bbox): number {
  return Math.max(bbox[2] - bbox[0], bbox[3] - bbox[1]);
}

/** Where the height post stands: out to the right of the front right
 * corner, so its handles clear the corner handle. */
export function postOf(bbox: Bbox): XY {
  return { x: bbox[2] + spanOf(bbox) * 0.12, y: bbox[1] };
}

/** The handles as they show on the canvas. */
export interface ShownFrame {
  outline: XY[];
  handles: { handle: AnyHandle; at: XY }[];
  stalk: XY | null;
  post: { from: XY; to: XY } | null;
}

/** Screen point of job XY at a scene height, or null behind the camera. */
export type Project = (x: number, y: number, sceneY: number) => XY | null;

/** Where the handles of `frame` show, or null when any is behind the camera. */
export function layoutFrame(project: Project, frame: TransformFrame): ShownFrame | null {
  const { bbox, params, planeY } = frame;
  const place = (p: XY, sceneY = planeY): XY | null => {
    const moved = applyParams(bbox, params, p);
    return project(moved.x, moved.y, sceneY);
  };
  const outline: XY[] = [];
  for (const index of CORNERS) {
    const at = place(cornerOf(bbox, index));
    if (at === null) {
      return null;
    }
    outline.push(at);
  }
  const handles: ShownFrame['handles'] = [];
  for (const handle of HANDLES) {
    const centre = handle.kind === 'move' && frame.centreMode ? frame.pivot : null;
    const at = place(centre ?? handlePoint(bbox, handle));
    if (at === null) {
      return null;
    }
    handles.push({ handle, at });
  }
  const corner = postOf(bbox);
  const span = spanOf(bbox);
  for (const kind of HEIGHT_KINDS) {
    const at = place(corner, frame.zeroY + heightOf(kind, frame.heights, span));
    if (at === null) {
      return null;
    }
    handles.push({ handle: { kind }, at });
  }
  if (frame.profile !== null) {
    const at = place(profilePointOf(bbox), frame.zeroY + profileHeightOf(frame.heights));
    if (at === null) {
      return null;
    }
    handles.push({ handle: { kind: 'profile' }, at });
  }
  const foot = place(corner, frame.zeroY);
  const head = place(corner, frame.zeroY + heightOf('total', frame.heights, span));
  return {
    outline,
    handles,
    stalk: place(edgeOf(bbox, 2)),
    post: foot === null || head === null ? null : { from: foot, to: head },
  };
}
