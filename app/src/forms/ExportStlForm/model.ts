// Pure state for the Export Model form: how far the mesh is reduced (Limit
// Triangles By a tolerance or a triangle budget) and how its back is
// closed. The mesh format (STL, OBJ or POV-Ray) is the file type chosen
// when saving.

import type { BackFace, MeshOptions } from '../../bridge/exports';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

/** Default Approximate Tolerance, mm. */
export const DEFAULT_TOLERANCE_MM = 0.01;
/** Default Maximum Triangle Count. */
export const DEFAULT_TRIANGLES = 1_000_000;

/** Mesh formats Save Triangulation writes, picked by the file type. */
export type MeshExportFormat = 'stl' | 'obj' | 'pov';

export const MESH_EXPORT_FORMATS: readonly MeshExportFormat[] = ['stl', 'obj', 'pov'];

/** The mesh format of a chosen file name: its extension, STL otherwise. */
export function meshFormatOf(path: string): MeshExportFormat {
  const ext = path.split(/[\\/]/).pop()?.split('.').pop()?.toLowerCase() ?? '';
  return MESH_EXPORT_FORMATS.find((f) => f === ext) ?? 'stl';
}

/** How the triangle count is limited. */
export type ReduceBy = 'tolerance' | 'count';

/** The job kinds that add a back of their own. */
export type JobKindForBacks = 'singleSided' | 'doubleSided' | 'rotary';

/** The backs a job can close with: the other side only on a double sided
 * job; a rotary model is left open or has its sides (end caps) closed. */
export function backFaces(kind: JobKindForBacks): readonly BackFace[] {
  if (kind === 'rotary') {
    return ['open', 'sides'];
  }
  const plain: BackFace[] = ['open', 'inverted', 'flat'];
  return kind === 'doubleSided' ? [...plain, 'otherSide'] : plain;
}

/** The back to use: one the job does not offer falls back to the flat
 * plane (the closed sides on a rotary job). */
export function shownBack(back: BackFace, kind: JobKindForBacks): BackFace {
  if (backFaces(kind).includes(back)) {
    return back;
  }
  return kind === 'rotary' ? 'sides' : 'flat';
}

/** A rotary model is meshed at full quality: no triangle limit applies. */
export function limitsTriangles(kind: JobKindForBacks): boolean {
  return kind !== 'rotary';
}

export interface ExportStlState {
  reduceBy: ReduceBy;
  tolerance: string;
  count: string;
  back: BackFace;
}

export function initialExportStlState(units: Units): ExportStlState {
  const inch = units === 'inch';
  return {
    reduceBy: 'tolerance',
    tolerance: lengthText(inch ? 0.0127 : DEFAULT_TOLERANCE_MM, units),
    count: String(DEFAULT_TRIANGLES),
    back: 'flat',
  };
}

export function convertExportStlUnits(
  state: ExportStlState,
  from: Units,
  to: Units,
): ExportStlState {
  return convertLengthFields(state, ['tolerance'], from, to);
}

export function toleranceError(state: ExportStlState, units: Units): string | undefined {
  if (state.reduceBy !== 'tolerance') {
    return undefined;
  }
  return fieldError(lengthValue(state.tolerance, units), { positive: true });
}

export function countError(state: ExportStlState): string | undefined {
  if (state.reduceBy !== 'count') {
    return undefined;
  }
  return fieldError(numberValue(state.count), { min: 2, integer: true });
}

/** What the kernel is asked to mesh, or null while a field is not valid. */
export function meshOptions(state: ExportStlState, units: Units): MeshOptions | null {
  if (toleranceError(state, units) !== undefined || countError(state) !== undefined) {
    return null;
  }
  if (state.reduceBy === 'count') {
    return { back: state.back, maxTriangles: numberValue(state.count) ?? DEFAULT_TRIANGLES };
  }
  return {
    back: state.back,
    tolerance: lengthValue(state.tolerance, units) ?? DEFAULT_TOLERANCE_MM,
  };
}
