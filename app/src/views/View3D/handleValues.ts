// The value box beside the active transform handle in the 3D view: the
// values the handle changes, live while dragging, and typed values turned
// back into a transform or new heights. Lengths are millimetres.

import type { ComponentTransformParams } from '../../bridge/model';
import { typedHeights, type HeightKind, type Heights } from './heightHandles';
import {
  applyParams,
  centreOf,
  cornerOf,
  edgeOf,
  type Bbox,
  type HandleKind,
  type XY,
} from './transformHandles';

/** Any handle of the 3D view: a box handle, a height handle or the
 * profile angle handle of a round or angled shape. */
export type AnyHandle = HandleKind | { kind: HeightKind } | { kind: 'profile' };

export type ValueKey =
  'x' | 'y' | 'width' | 'height' | 'angle' | 'base' | 'shape' | 'total' | 'profile';

export interface ValueField {
  key: ValueKey;
  value: number;
  /** How the box shows it: a length in the job's unit, percent or degrees. */
  unit: 'length' | 'percent' | 'angle';
}

/** What the handles show now. */
export interface HandleState {
  bbox: Bbox;
  /** The drag so far on the box handles. */
  params: ComponentTransformParams;
  /** The rotation centre, null for the middle of the box. */
  pivot: XY | null;
  /** Middle handle moves the rotation centre instead of the component. */
  centreMode: boolean;
  heights: Heights;
  /** The shape's edge angle as the drag leaves it, null for no shape. */
  profile: number | null;
}

export function isHeightHandle(handle: AnyHandle): handle is { kind: HeightKind } {
  return handle.kind === 'base' || handle.kind === 'shape' || handle.kind === 'total';
}

export function isProfileHandle(handle: AnyHandle): handle is { kind: 'profile' } {
  return handle.kind === 'profile';
}

function opposite(index: 0 | 1 | 2 | 3): 0 | 1 | 2 | 3 {
  return ((index + 2) % 4) as 0 | 1 | 2 | 3;
}

function horizontal(index: 0 | 1 | 2 | 3): boolean {
  return index === 1 || index === 3;
}

/** The fields of the value box for `handle`. */
export function valueFields(handle: AnyHandle, state: HandleState): ValueField[] {
  const { bbox, params, heights } = state;
  const width = (bbox[2] - bbox[0]) * params.scaleX;
  const height = (bbox[3] - bbox[1]) * params.scaleY;
  switch (handle.kind) {
    case 'move': {
      const at =
        state.centreMode && state.pivot !== null
          ? state.pivot
          : applyParams(bbox, params, centreOf(bbox));
      return [
        { key: 'x', value: at.x, unit: 'length' },
        { key: 'y', value: at.y, unit: 'length' },
      ];
    }
    case 'corner':
      return [
        { key: 'width', value: width, unit: 'length' },
        { key: 'height', value: height, unit: 'length' },
      ];
    case 'edge':
      return horizontal(handle.index)
        ? [{ key: 'width', value: width, unit: 'length' }]
        : [{ key: 'height', value: height, unit: 'length' }];
    case 'rotate':
      return [{ key: 'angle', value: params.rotateDeg, unit: 'angle' }];
    case 'base':
      return [{ key: 'base', value: heights.base, unit: 'length' }];
    case 'shape':
      return [{ key: 'shape', value: heights.scale * 100, unit: 'percent' }];
    case 'total':
      return [{ key: 'total', value: heights.base + heights.shape, unit: 'length' }];
    case 'profile':
      return [{ key: 'profile', value: state.profile ?? 0, unit: 'angle' }];
  }
}

export type TypedValues = Partial<Record<ValueKey, number>>;

/** What typed values ask a box handle for. */
export type BoxResult =
  { type: 'transform'; params: ComponentTransformParams } | { type: 'pivot'; pivot: XY } | null;

const IDENTITY: ComponentTransformParams = {
  offset: { x: 0, y: 0 },
  scaleX: 1,
  scaleY: 1,
  rotateDeg: 0,
};

function ratio(value: number | undefined, current: number): number | null {
  if (value === undefined) {
    return 1;
  }
  if (!Number.isFinite(value) || value <= 0 || current <= 1e-9) {
    return null;
  }
  return value / current;
}

/** Typed values on a box handle, applied to the component as it stands
 * (no drag in progress). Null when a value is not usable. */
export function typedTransform(
  handle: HandleKind,
  bbox: Bbox,
  pivot: XY | null,
  centreMode: boolean,
  values: TypedValues,
): BoxResult {
  const width = bbox[2] - bbox[0];
  const height = bbox[3] - bbox[1];
  switch (handle.kind) {
    case 'move': {
      const from = centreMode && pivot !== null ? pivot : centreOf(bbox);
      const x = values.x ?? from.x;
      const y = values.y ?? from.y;
      if (!Number.isFinite(x) || !Number.isFinite(y)) {
        return null;
      }
      if (centreMode) {
        return { type: 'pivot', pivot: { x, y } };
      }
      return {
        type: 'transform',
        params: { ...IDENTITY, offset: { x: x - from.x, y: y - from.y } },
      };
    }
    case 'corner': {
      const sx = ratio(values.width, width);
      const sy = ratio(values.height, height);
      if (sx === null || sy === null) {
        return null;
      }
      const centre = cornerOf(bbox, opposite(handle.index));
      return { type: 'transform', params: { ...IDENTITY, scaleX: sx, scaleY: sy, centre } };
    }
    case 'edge': {
      const centre = edgeOf(bbox, opposite(handle.index));
      if (horizontal(handle.index)) {
        const sx = ratio(values.width, width);
        return sx === null
          ? null
          : { type: 'transform', params: { ...IDENTITY, scaleX: sx, centre } };
      }
      const sy = ratio(values.height, height);
      return sy === null
        ? null
        : { type: 'transform', params: { ...IDENTITY, scaleY: sy, centre } };
    }
    case 'rotate': {
      const angle = values.angle ?? 0;
      if (!Number.isFinite(angle)) {
        return null;
      }
      return {
        type: 'transform',
        params: { ...IDENTITY, rotateDeg: angle, centre: pivot ?? centreOf(bbox) },
      };
    }
  }
}

/** Typed values on a height handle, or null when not usable. */
export function typedHeightValues(
  kind: HeightKind,
  heights: Heights,
  values: TypedValues,
): Heights | null {
  const value = values[kind];
  return value === undefined ? null : typedHeights(kind, heights, value);
}
