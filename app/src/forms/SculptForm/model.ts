// Pure state for the Sculpt form: the brush and its settings. Strength and
// softness are entered as percentages; lengths follow the display unit.

import type { Brush, BrushParams, CustomBrushParams, SculptMode } from '../../bridge/model';
import type { SculptPreset } from '../../bridge/prefs';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

/** Files Load Brush offers: our clips and pictures. */
export const BRUSH_EXTENSIONS: readonly string[] = [
  'carveclip',
  'png',
  'jpg',
  'jpeg',
  'bmp',
  'webp',
  'gif',
  'tif',
  'tiff',
];

/** The brush list's value for the loaded brush file. */
export const FILE_BRUSH = 'file';

/** The sculpting tools in the order of their keys, 1 to 5. Shift with the
 * Undo Brush erases instead. */
export const BRUSHES: readonly Brush[] = ['smooth', 'smudge', 'deposit', 'remove', 'restore'];

export const MODES: readonly SculptMode[] = ['normal', 'raise', 'lower'];

export interface SculptFormState {
  brush: Brush;
  diameter: string;
  /** Percent, 1 to 100. */
  strength: string;
  /** Percent, 0 (hard edge) to 100 (fully soft). */
  softness: string;
  /** Keep the sculpting inside the component's edge. */
  preserve: boolean;
  mode: SculptMode;
  /** Custom Brush: sculpt with another component's shape. */
  useCustom: boolean;
  /** Id of the brush component, as the list holds it, FILE_BRUSH for the
   * loaded brush file, '' for none. */
  customBrush: string;
  /** The brush file loaded with Load Brush, '' for none. */
  brushFile: string;
  /** Degrees. */
  angle: string;
  dynamic: boolean;
  merge: boolean;
}

export type SculptField = 'diameter' | 'strength' | 'softness' | 'angle';
export type SculptErrors = Partial<Record<SculptField, string>>;

export function initialSculptState(units: Units): SculptFormState {
  return {
    brush: 'deposit',
    diameter: lengthText(10, units),
    strength: '50',
    softness: '50',
    preserve: true,
    mode: 'normal',
    useCustom: false,
    customBrush: '',
    brushFile: '',
    angle: '0',
    dynamic: false,
    merge: true,
  };
}

export function convertSculptUnits(
  state: SculptFormState,
  from: Units,
  to: Units,
): SculptFormState {
  return convertLengthFields(state, ['diameter'], from, to);
}

/** Whether the brush builds up or cuts away (Deposit or Remove), the tools
 * a custom brush shape works with. */
export function buildsOrCuts(brush: Brush): boolean {
  return brush === 'deposit' || brush === 'remove';
}

export function validateSculpt(state: SculptFormState, units: Units): SculptErrors {
  const errors: SculptErrors = {};
  const set = (field: SculptField, error: string | undefined) => {
    if (error !== undefined) {
      errors[field] = error;
    }
  };
  set('diameter', fieldError(lengthValue(state.diameter, units), { positive: true }));
  set('strength', fieldError(numberValue(state.strength), { min: 1, max: 100 }));
  set('softness', fieldError(numberValue(state.softness), { min: 0, max: 100 }));
  if (usesCustom(state)) {
    set('angle', fieldError(numberValue(state.angle), { min: -360, max: 360 }));
  }
  return errors;
}

/** The brush the kernel takes, or null while a field is invalid. */
export function brushParams(state: SculptFormState, units: Units): BrushParams | null {
  if (Object.keys(validateSculpt(state, units)).length > 0) {
    return null;
  }
  const diameter = lengthValue(state.diameter, units) ?? 0;
  const strength = (numberValue(state.strength) ?? 0) / 100;
  const falloff = (numberValue(state.softness) ?? 0) / 100;
  return {
    brush: state.brush,
    diameter,
    strength,
    falloff,
    grow: !state.preserve,
    mode: state.mode,
  };
}

/** The tool for number key `key` (1 to 5), if it is one. */
export function toolForKey(key: string): Brush | null {
  const n = Number(key);
  return Number.isInteger(n) && n >= 1 && n <= BRUSHES.length ? (BRUSHES[n - 1] ?? null) : null;
}

/** The settings a preset keeps, lengths in millimetres. */
export function presetSettings(state: SculptFormState, units: Units): Record<string, unknown> {
  return {
    diameter: lengthValue(state.diameter, units) ?? 10,
    strength: numberValue(state.strength) ?? 50,
    softness: numberValue(state.softness) ?? 50,
    preserve: state.preserve,
    mode: state.mode,
  };
}

function num(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

/** The form with a preset's settings (and its tool). */
export function withPreset(
  state: SculptFormState,
  preset: SculptPreset,
  units: Units,
): SculptFormState {
  const s = preset.settings;
  const tool = BRUSHES.find((b) => b === preset.tool) ?? state.brush;
  const mode = MODES.find((m) => m === s['mode']) ?? state.mode;
  return {
    ...state,
    brush: tool,
    diameter: lengthText(num(s['diameter'], 10), units),
    strength: String(num(s['strength'], 50)),
    softness: String(num(s['softness'], 50)),
    preserve: s['preserve'] !== false,
    mode,
  };
}

/** The presets with `preset` added, replacing one of the same name and
 * tool. */
export function addPreset(list: readonly SculptPreset[], preset: SculptPreset): SculptPreset[] {
  return [...list.filter((p) => !(p.name === preset.name && p.tool === preset.tool)), preset];
}

/** Whether a preset of that name is already kept for the tool. */
export function hasPreset(list: readonly SculptPreset[], name: string, tool: string): boolean {
  return list.some((p) => p.name === name && p.tool === tool);
}

export function removePreset(
  list: readonly SculptPreset[],
  name: string,
  tool: string,
): SculptPreset[] {
  return list.filter((p) => !(p.name === name && p.tool === tool));
}

/** Diameter change per wheel notch, as a fraction; the fine step is used
 * with Ctrl or Cmd. */
const DIAMETER_STEP = 0.1;
const FINE_DIAMETER_STEP = 0.02;
/** Strength change per wheel notch, percent. */
const STRENGTH_STEP = 5;
/** Custom brush turn per wheel notch with R held, degrees; fine with Ctrl
 * or Cmd. */
const ANGLE_STEP = 15;
const FINE_ANGLE_STEP = 1;

/** An angle kept within -180 to 180 degrees. */
function wrapAngle(degrees: number): number {
  const wrapped = ((((degrees + 180) % 360) + 360) % 360) - 180;
  return wrapped === -180 ? 180 : wrapped;
}

/** The form after a wheel change made in the 3D view. Invalid fields are
 * left alone. */
export function adjustSculpt(
  state: SculptFormState,
  adjust: { kind: 'diameter' | 'strength' | 'angle'; steps: number; fine: boolean },
  units: Units,
): SculptFormState {
  if (adjust.kind === 'angle') {
    const angle = numberValue(state.angle);
    if (angle === null) {
      return state;
    }
    const step = adjust.fine ? FINE_ANGLE_STEP : ANGLE_STEP;
    return { ...state, angle: String(wrapAngle(angle + adjust.steps * step)) };
  }
  if (adjust.kind === 'strength') {
    const strength = numberValue(state.strength);
    if (strength === null) {
      return state;
    }
    const next = Math.min(100, Math.max(1, Math.round(strength + adjust.steps * STRENGTH_STEP)));
    return { ...state, strength: String(next) };
  }
  const diameter = lengthValue(state.diameter, units);
  if (diameter === null || diameter <= 0) {
    return state;
  }
  const step = adjust.fine ? FINE_DIAMETER_STEP : DIAMETER_STEP;
  const next = Math.max(0.1, diameter * (1 + step) ** adjust.steps);
  return { ...state, diameter: lengthText(next, units) };
}

/** Whether the custom brush is in use: on, chosen, and Deposit or Remove. */
export function usesCustom(state: SculptFormState): boolean {
  return state.useCustom && state.customBrush !== '' && buildsOrCuts(state.brush);
}

/** The custom brush the kernel takes, or null for the round brush. */
export function customBrushParams(state: SculptFormState): CustomBrushParams | null {
  const angle = numberValue(state.angle);
  if (!usesCustom(state) || angle === null) {
    return null;
  }
  const hold = { angle, dynamic: state.dynamic, merge: state.merge };
  if (state.customBrush === FILE_BRUSH) {
    return state.brushFile === '' ? null : { file: state.brushFile, ...hold };
  }
  const brush = Number(state.customBrush);
  return Number.isInteger(brush) ? { brush, ...hold } : null;
}

/** The form after Load Brush chose `path`: the file is the brush. */
export function withBrushFile(state: SculptFormState, path: string): SculptFormState {
  return { ...state, useCustom: true, brushFile: path, customBrush: FILE_BRUSH };
}

/** The file name of a brush path, without folders or extension. */
export function brushFileName(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? path;
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(0, dot) : name;
}
