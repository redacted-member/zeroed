// Pure state for what Sketch Carving and Laser Sketch trace: a placed bitmap
// or the 3D model, the machining boundary and the line thickness. The
// bitmap is the one selected in the 2D view; toolpaths saved with an image
// file placed by numbers keep that file and its placement.
// Lengths are text in the job unit.

import type { EntitySummary } from '../../bridge/document';
import type { SketchBoundary, SketchSource, SketchTrace } from '../../bridge/sketch';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';

export interface SketchSourceState {
  source: SketchSource;
  /** A bitmap entity placed in the job, found in the selection. */
  bitmap: number | null;
  /** An image file, used when no bitmap is placed. */
  path: string;
  left: string;
  bottom: string;
  width: string;
  /** Empty keeps the image's proportions. */
  height: string;
  boundary: SketchBoundary;
  /** The model level for a Selected Level boundary. */
  level: number | null;
  boundaryOffset: string;
  /** 0 to 100. */
  lineThickness: number;
}

export type SketchSourceField =
  'path' | 'left' | 'bottom' | 'width' | 'height' | 'boundaryOffset' | 'boundary';
export type SketchSourceErrors = Partial<Record<SketchSourceField, string>>;

const LENGTHS: Array<'left' | 'bottom' | 'width' | 'height' | 'boundaryOffset'> = [
  'left',
  'bottom',
  'width',
  'height',
  'boundaryOffset',
];

export function initialSketchSource(units: Units): SketchSourceState {
  return {
    source: 'bitmap',
    bitmap: null,
    path: '',
    left: lengthText(0, units),
    bottom: lengthText(0, units),
    width: lengthText(units === 'inch' ? 101.6 : 100, units),
    height: '',
    boundary: 'source',
    level: null,
    boundaryOffset: lengthText(0, units),
    lineThickness: 50,
  };
}

export function convertSketchSource(
  state: SketchSourceState,
  from: Units,
  to: Units,
): SketchSourceState {
  return convertLengthFields(state, LENGTHS, from, to);
}

/** Whether the image file and its placement are used (no placed bitmap). */
export function usesFile(state: SketchSourceState): boolean {
  return state.source === 'bitmap' && state.bitmap === null;
}

/** Whether the numbers placing an image file show: only for a toolpath
 * that was saved with a file instead of a placed bitmap. */
export function showsFilePlacement(state: SketchSourceState): boolean {
  return usesFile(state) && state.path.trim() !== '';
}

/** The source with another boundary; Selected Level starts on the active level. */
export function withBoundary(
  state: SketchSourceState,
  boundary: SketchBoundary,
  activeLevel: number | null,
): SketchSourceState {
  const level = state.level ?? (boundary === 'level' ? activeLevel : null);
  return { ...state, boundary, level };
}

/** The first bitmap among the selected objects. */
export function selectedBitmap(ids: number[], entities: EntitySummary[]): number | null {
  const kinds = new Map(entities.map((e) => [e.id, e.kind]));
  return ids.find((id) => kinds.get(id) === 'bitmap') ?? null;
}

/** Selected objects that are closed vectors, for a vector boundary. */
export function boundaryIds(ids: number[], entities: EntitySummary[]): number[] {
  const byId = new Map(entities.map((e) => [e.id, e]));
  return ids.filter((id) => {
    const e = byId.get(id);
    return e !== undefined && e.kind !== 'bitmap' && e.closed;
  });
}

export function validateSketchSource(
  state: SketchSourceState,
  units: Units,
  boundaryCount: number,
): SketchSourceErrors {
  const errors: SketchSourceErrors = {};
  if (usesFile(state)) {
    if (state.path.trim() === '') {
      errors.path = strings.toolpaths.sketch.pickBitmap;
    }
    const left = fieldError(lengthValue(state.left, units));
    const bottom = fieldError(lengthValue(state.bottom, units));
    const width = fieldError(lengthValue(state.width, units), { positive: true });
    const height =
      state.height.trim() === ''
        ? undefined
        : fieldError(lengthValue(state.height, units), { positive: true });
    if (left !== undefined) {
      errors.left = left;
    }
    if (bottom !== undefined) {
      errors.bottom = bottom;
    }
    if (width !== undefined) {
      errors.width = width;
    }
    if (height !== undefined) {
      errors.height = height;
    }
  }
  // The offset grows whichever boundary is chosen.
  const offset = fieldError(lengthValue(state.boundaryOffset, units), { min: 0 });
  if (offset !== undefined) {
    errors.boundaryOffset = offset;
  }
  if (state.boundary === 'vector' && boundaryCount === 0) {
    errors.boundary = strings.toolpaths.sketch.needsBoundary;
  }
  if (state.boundary === 'level' && state.level === null) {
    errors.boundary = strings.toolpaths.sketch.needsLevel;
  }
  return errors;
}

/** The kernel's trace settings, or null while a field is invalid. */
export function sketchTrace(
  state: SketchSourceState,
  units: Units,
  boundaryCount: number,
): SketchTrace | null {
  if (Object.keys(validateSketchSource(state, units, boundaryCount)).length > 0) {
    return null;
  }
  const file = usesFile(state);
  return {
    source: state.source,
    bitmap: state.source === 'bitmap' ? state.bitmap : null,
    path: file ? state.path : '',
    origin: {
      x: lengthValue(state.left, units) ?? 0,
      y: lengthValue(state.bottom, units) ?? 0,
    },
    width: lengthValue(state.width, units) ?? 100,
    height: state.height.trim() === '' ? 0 : (lengthValue(state.height, units) ?? 0),
    boundary: state.boundary,
    level: state.boundary === 'level' ? state.level : null,
    boundaryOffset: lengthValue(state.boundaryOffset, units) ?? 0,
    lineThickness: Math.min(100, Math.max(0, Math.round(state.lineThickness))),
  };
}

/** The objects a sketch toolpath records: the placed bitmap, so moving it
 *  makes the toolpath out of date, and the boundary vectors. */
export function sketchIds(state: SketchSourceState, boundary: number[]): number[] {
  const ids = state.boundary === 'vector' ? [...boundary] : [];
  if (state.source === 'bitmap' && state.bitmap !== null && !ids.includes(state.bitmap)) {
    ids.unshift(state.bitmap);
  }
  return ids;
}

export function sketchSourceFrom(trace: SketchTrace, units: Units): SketchSourceState {
  return {
    source: trace.source,
    bitmap: trace.bitmap ?? null,
    path: trace.path,
    left: lengthText(trace.origin.x, units),
    bottom: lengthText(trace.origin.y, units),
    width: lengthText(trace.width, units),
    height: trace.height > 0 ? lengthText(trace.height, units) : '',
    boundary: trace.boundary,
    level: trace.level ?? null,
    boundaryOffset: lengthText(trace.boundaryOffset, units),
    lineThickness: trace.lineThickness,
  };
}
