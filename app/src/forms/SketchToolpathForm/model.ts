// Pure state for the Sketch Carving Toolpath form: what to trace, the start
// and optional flat depth, and the vector selection.

import type { Tool, ToolKind } from '../../bridge/cam';
import type { SketchCarveParams, VectorFilter } from '../../bridge/sketch';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import {
  convertSketchSource,
  initialSketchSource,
  sketchSourceFrom,
  sketchTrace,
  validateSketchSource,
  type SketchSourceErrors,
  type SketchSourceState,
} from '../SketchSourceFields/model';
import { withSelector } from '../VectorSelectionField/model';

export interface SketchFormState {
  name: string;
  source: SketchSourceState;
  startDepth: string;
  useFlatDepth: boolean;
  flatDepth: string;
  selector: VectorFilter | null;
}

export type SketchFormErrors = SketchSourceErrors &
  Partial<Record<'startDepth' | 'flatDepth', string>>;

/** The tools Sketch Carving cuts with; its Select ... lists only these. */
export const SKETCH_TOOL_KINDS: readonly ToolKind[] = [
  'vBit',
  'engravingTool',
  'ballNose',
  'taperedBallNose',
];

/** Whether Sketch Carving can cut with the tool. */
export function isCarvingTool(tool: Tool | null): boolean {
  return tool !== null && SKETCH_TOOL_KINDS.includes(tool.toolType.kind);
}

export function initialSketchState(units: Units): SketchFormState {
  return {
    name: '',
    source: initialSketchSource(units),
    startDepth: lengthText(0, units),
    useFlatDepth: false,
    flatDepth: lengthText(units === 'inch' ? 1.27 : 1, units),
    selector: null,
  };
}

export function convertSketchUnits(
  state: SketchFormState,
  from: Units,
  to: Units,
): SketchFormState {
  const next = convertLengthFields(state, ['startDepth', 'flatDepth'], from, to);
  return { ...next, source: convertSketchSource(state.source, from, to) };
}

export function validateSketch(
  state: SketchFormState,
  units: Units,
  boundaryCount: number,
): SketchFormErrors {
  const errors: SketchFormErrors = validateSketchSource(state.source, units, boundaryCount);
  const start = fieldError(lengthValue(state.startDepth, units), { min: 0 });
  if (start !== undefined) {
    errors.startDepth = start;
  }
  if (state.useFlatDepth) {
    const flat = fieldError(lengthValue(state.flatDepth, units), { positive: true });
    if (flat !== undefined) {
      errors.flatDepth = flat;
    }
  }
  return errors;
}

export function sketchParams(
  state: SketchFormState,
  units: Units,
  boundaryCount: number,
): SketchCarveParams | null {
  if (Object.keys(validateSketch(state, units, boundaryCount)).length > 0) {
    return null;
  }
  const trace = sketchTrace(state.source, units, boundaryCount);
  if (trace === null) {
    return null;
  }
  return withSelector(
    {
      ...trace,
      startDepth: lengthValue(state.startDepth, units) ?? 0,
      flatDepth: state.useFlatDepth ? lengthValue(state.flatDepth, units) : null,
    },
    state.selector,
  );
}

export function sketchStateFrom(
  params: SketchCarveParams,
  name: string,
  units: Units,
): SketchFormState {
  const base = initialSketchState(units);
  return {
    name,
    source: sketchSourceFrom(params, units),
    startDepth: lengthText(params.startDepth, units),
    useFlatDepth: params.flatDepth !== null,
    flatDepth: params.flatDepth === null ? base.flatDepth : lengthText(params.flatDepth, units),
    selector: params.selector ?? null,
  };
}
