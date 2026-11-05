// Pure state for the Thread Milling Toolpath form.

import type { ThreadMillingParams, Tool } from '../../bridge/cam';
import type { EntitySummary, Path } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';

/** A standard thread: its designation, major diameter and pitch in millimetres. */
export interface ThreadPreset {
  id: string;
  diameter: number;
  pitch: number;
}

const INCH = 25.4;

/** ISO metric coarse and common fine threads. */
export const METRIC_PRESETS: readonly ThreadPreset[] = [
  ['M3', 3, 0.5],
  ['M4', 4, 0.7],
  ['M5', 5, 0.8],
  ['M6', 6, 1],
  ['M8', 8, 1.25],
  ['M8 x 1', 8, 1],
  ['M10', 10, 1.5],
  ['M10 x 1.25', 10, 1.25],
  ['M12', 12, 1.75],
  ['M12 x 1.5', 12, 1.5],
  ['M14', 14, 2],
  ['M16', 16, 2],
  ['M16 x 1.5', 16, 1.5],
  ['M18', 18, 2.5],
  ['M20', 20, 2.5],
  ['M22', 22, 2.5],
  ['M24', 24, 3],
  ['M27', 27, 3],
  ['M30', 30, 3.5],
  ['M36', 36, 4],
  ['M42', 42, 4.5],
  ['M48', 48, 5],
].map(([id, diameter, pitch]) => ({
  id: String(id),
  diameter: Number(diameter),
  pitch: Number(pitch),
}));

/** Unified (inch) coarse and fine threads: diameter in inches, threads per inch. */
export const UNIFIED_PRESETS: readonly ThreadPreset[] = [
  ['#4-40 UNC', 0.112, 40],
  ['#6-32 UNC', 0.138, 32],
  ['#8-32 UNC', 0.164, 32],
  ['#10-24 UNC', 0.19, 24],
  ['#10-32 UNF', 0.19, 32],
  ['1/4-20 UNC', 0.25, 20],
  ['1/4-28 UNF', 0.25, 28],
  ['5/16-18 UNC', 0.3125, 18],
  ['3/8-16 UNC', 0.375, 16],
  ['3/8-24 UNF', 0.375, 24],
  ['7/16-14 UNC', 0.4375, 14],
  ['1/2-13 UNC', 0.5, 13],
  ['1/2-20 UNF', 0.5, 20],
  ['5/8-11 UNC', 0.625, 11],
  ['3/4-10 UNC', 0.75, 10],
  ['7/8-9 UNC', 0.875, 9],
  ['1-8 UNC', 1, 8],
  ['1-1/4-7 UNC', 1.25, 7],
  ['1-1/2-6 UNC', 1.5, 6],
].map(([id, inches, tpi]) => ({
  id: String(id),
  diameter: Number(inches) * INCH,
  pitch: INCH / Number(tpi),
}));

export function findPreset(id: string): ThreadPreset | undefined {
  return [...METRIC_PRESETS, ...UNIFIED_PRESETS].find((preset) => preset.id === id);
}

/** The presets a job offers: ISO metric for a metric job, Unified for an inch one. */
export function presetsFor(units: Units): readonly ThreadPreset[] {
  return units === 'inch' ? UNIFIED_PRESETS : METRIC_PRESETS;
}

/** The fit tolerance an external preset fills in: a tenth of its pitch, in mm. */
export function externalFit(pitch: number): number {
  return Math.round((pitch / 10) * 1000) / 1000;
}

/** A multi point thread mill's pitch and the thread length its teeth cover,
 * in millimetres; null for a single point cutter or another tool. */
export function multiPoint(tool: Tool | null): { pitch: number; reach: number } | null {
  if (tool?.toolType.kind !== 'threadMill') {
    return null;
  }
  const teeth = tool.toolType['teeth'];
  const pitch = tool.toolType['pitch'];
  if (typeof teeth !== 'number' || teeth <= 1 || typeof pitch !== 'number' || !(pitch > 0)) {
    return null;
  }
  return { pitch, reach: teeth * pitch };
}

export interface ThreadFormState {
  name: string;
  startDepth: string;
  maxDepth: string;
  /** Preset id, or '' for a custom size. */
  preset: string;
  pitch: string;
  diameter: string;
  fitTolerance: string;
  kind: ThreadMillingParams['kind'];
  hand: ThreadMillingParams['hand'];
  cut: ThreadMillingParams['cut'];
  selectionOrder: boolean;
}

export type ThreadField = 'startDepth' | 'maxDepth' | 'pitch' | 'diameter' | 'fitTolerance';
export type ThreadErrors = Partial<Record<ThreadField, string>>;

export function initialThreadState(units: Units): ThreadFormState {
  const preset = units === 'inch' ? '1/2-13 UNC' : 'M10';
  const found = findPreset(preset);
  return {
    name: '',
    startDepth: lengthText(0, units),
    maxDepth: lengthText(units === 'inch' ? 12.7 : 10, units),
    preset,
    pitch: lengthText(found?.pitch ?? 1.5, units),
    diameter: lengthText(found?.diameter ?? 10, units),
    // The form shows 0.01 inch.
    fitTolerance: lengthText(units === 'inch' ? 0.254 : 0.25, units),
    kind: 'internal',
    hand: 'right',
    cut: 'up',
    selectionOrder: false,
  };
}

export function convertThreadUnits(
  state: ThreadFormState,
  from: Units,
  to: Units,
): ThreadFormState {
  return convertLengthFields(
    state,
    ['startDepth', 'maxDepth', 'pitch', 'diameter', 'fitTolerance'],
    from,
    to,
  );
}

/** The form with a preset's diameter and pitch filled in, and for an
 * external thread the fit tolerance its pitch wants. */
export function applyPreset(state: ThreadFormState, id: string, units: Units): ThreadFormState {
  const preset = findPreset(id);
  if (preset === undefined) {
    return { ...state, preset: '' };
  }
  return {
    ...state,
    preset: id,
    diameter: lengthText(preset.diameter, units),
    pitch: lengthText(preset.pitch, units),
    fitTolerance:
      state.kind === 'external' ? lengthText(externalFit(preset.pitch), units) : state.fitTolerance,
  };
}

/** The form cutting with `tool`: a multi point cutter sets the pitch. */
export function withTool(state: ThreadFormState, tool: Tool | null, units: Units): ThreadFormState {
  const multi = multiPoint(tool);
  if (multi === null) {
    return state;
  }
  const pitch = lengthText(multi.pitch, units);
  const preset = findPreset(state.preset);
  const keep = preset !== undefined && Math.abs(preset.pitch - multi.pitch) < 1e-6;
  const next = keep ? state.preset : '';
  return pitch === state.pitch && next === state.preset ? state : { ...state, pitch, preset: next };
}

export function validateThread(
  state: ThreadFormState,
  units: Units,
  tool: Tool | null = null,
): ThreadErrors {
  const errors: ThreadErrors = {};
  const multi = multiPoint(tool);
  const maxDepth = lengthValue(state.maxDepth, units);
  if (multi !== null && maxDepth !== null && threadLength(maxDepth, tool) > multi.reach + 1e-9) {
    errors.maxDepth = strings.toolpaths.thread.tooLong(
      lengthText(multi.reach, units),
      unitSuffix(units),
    );
  }
  const checks: Array<[ThreadField, string | undefined]> = [
    ['startDepth', fieldError(lengthValue(state.startDepth, units), { min: 0 })],
    ['maxDepth', fieldError(lengthValue(state.maxDepth, units), { positive: true })],
    ['pitch', fieldError(lengthValue(state.pitch, units), { positive: true })],
    ['diameter', fieldError(lengthValue(state.diameter, units), { positive: true })],
    ['fitTolerance', fieldError(lengthValue(state.fitTolerance, units))],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

export function threadParams(
  state: ThreadFormState,
  units: Units,
  tool: Tool | null = null,
): ThreadMillingParams | null {
  if (Object.keys(validateThread(state, units, tool)).length > 0) {
    return null;
  }
  return {
    startDepth: lengthValue(state.startDepth, units) ?? 0,
    maxDepth: lengthValue(state.maxDepth, units) ?? 0,
    pitch: lengthValue(state.pitch, units) ?? 0,
    diameter: lengthValue(state.diameter, units) ?? 0,
    fitTolerance: lengthValue(state.fitTolerance, units) ?? 0,
    kind: state.kind,
    hand: state.hand,
    cut: state.cut,
    selectionOrder: state.selectionOrder,
  };
}

export function threadStateFrom(
  params: ThreadMillingParams,
  name: string,
  units: Units,
): ThreadFormState {
  const preset = presetsFor(units).find(
    (p) => Math.abs(p.diameter - params.diameter) < 1e-6 && Math.abs(p.pitch - params.pitch) < 1e-6,
  );
  return {
    name,
    startDepth: lengthText(params.startDepth, units),
    maxDepth: lengthText(params.maxDepth, units),
    preset: preset?.id ?? '',
    pitch: lengthText(params.pitch, units),
    diameter: lengthText(params.diameter, units),
    fitTolerance: lengthText(params.fitTolerance, units),
    kind: params.kind,
    hand: params.hand,
    cut: params.cut,
    selectionOrder: params.selectionOrder ?? false,
  };
}

/** Height of a thread mill's lowest tooth above its bottom, in millimetres. */
export function toothOffsetOf(tool: Tool | null): number {
  const offset = tool?.toolType.kind === 'threadMill' ? tool.toolType['toothOffset'] : undefined;
  return typeof offset === 'number' && offset > 0 ? offset : 0;
}

/** Thread Length (L): the max depth less the tool's tooth offset, in mm. */
export function threadLength(maxDepthMm: number, tool: Tool | null): number {
  return maxDepthMm - toothOffsetOf(tool);
}

/** A circle of `diameter` round `(cx, cy)` as two half circle arcs. */
export function circlePath(cx: number, cy: number, diameter: number): Path {
  const r = diameter / 2;
  return {
    subpaths: [
      {
        start: { x: cx + r, y: cy },
        closed: true,
        segments: [
          { kind: 'arc', to: { x: cx - r, y: cy }, bulge: 1 },
          { kind: 'arc', to: { x: cx + r, y: cy }, bulge: 1 },
        ],
      },
    ],
  };
}

/** Circles of the minor diameter centred on the selected closed vectors,
 * the holes their internal threads need. */
export function holeCircles(
  params: ThreadMillingParams,
  selected: readonly EntitySummary[],
): Path[] {
  const minor = minorDiameter({ ...params, kind: 'internal' });
  if (!(minor > 0)) {
    return [];
  }
  return selected
    .filter((e) => e.kind === 'path' && e.closed)
    .map((e) => circlePath((e.bbox[0] + e.bbox[2]) / 2, (e.bbox[1] + e.bbox[3]) / 2, minor));
}

/** Minor diameter of the thread in millimetres, where the crests end: the
 * hole an internal thread needs, or the root of an external one. */
export function minorDiameter(params: ThreadMillingParams): number {
  const basic = (Math.sqrt(3) / 2) * params.pitch;
  const depth = params.kind === 'internal' ? (5 / 8) * basic : (17 / 24) * basic;
  return params.diameter - 2 * depth;
}

/** A problem with the picked tool, shown beside the tool picker. */
export function threadToolWarning(tool: Tool | null): string | undefined {
  return tool !== null && tool.toolType.kind !== 'threadMill'
    ? strings.toolpaths.thread.notThreadMill
    : undefined;
}
