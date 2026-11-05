// Pure state for the Slice Model form: one thickness for equal slices (up
// from the zero plane or down from the top) or a number of equal slices,
// always of the composite model, with an optional outline of each slice. The Slice Heights list holds the Z values
// the options cut at; editing it (Apply, Delete, a double click on the
// height bar) makes the list the slicing, until an option changes again.

import type { SliceArgs } from '../../bridge/modelTools';
import type { SliceBand, ViewHeightEdit } from '../../state/sliceViewStore';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { fieldError, lengthText, lengthValue, numberValue } from '../../lib/formFields';
import { formatLength } from '../../lib/units';

export type SliceMode = 'uniform' | 'count';

export interface SliceModelState {
  mode: SliceMode;
  thickness: string;
  /** Slice Thickness from the top of the model down, the part slice at
   * the bottom; otherwise up from the zero plane. */
  fromTop: boolean;
  /** Number of Slices, equal thickness. */
  count: string;
  /** Create Boundary Vectors: an outline of each slice on its own layer. */
  boundaryVectors: boolean;
  /** The edited Slice Heights list in millimetres, lowest first, or null
   * while the list follows the Model Slicing options. */
  heights: number[] | null;
  /** The list entry being edited, or null to add a new height. */
  activeHeight: number | null;
  /** The height typed under the list, in the display unit. */
  heightText: string;
}

export type SliceModelField = 'thickness' | 'count';
export type SliceModelErrors = Partial<Record<SliceModelField, string>>;

export function initialSliceModelState(units: Units): SliceModelState {
  const two = units === 'inch' ? 0.5 : 12;
  const toMm = units === 'inch' ? 25.4 : 1;
  return {
    mode: 'uniform',
    thickness: lengthText(two * toMm, units),
    fromTop: false,
    count: '2',
    boundaryVectors: false,
    heights: null,
    activeHeight: null,
    heightText: '',
  };
}

export function convertSliceModelUnits(
  state: SliceModelState,
  from: Units,
  to: Units,
): SliceModelState {
  const thickness = lengthValue(state.thickness, from);
  const height = lengthValue(state.heightText, from);
  return {
    ...state,
    heightText: height === null ? state.heightText : lengthText(height, to),
    thickness: thickness === null ? state.thickness : lengthText(thickness, to),
  };
}

/** Most slices the kernel makes in one go. */
export const MAX_SLICES = 200;

export function validateSliceModel(state: SliceModelState, units: Units): SliceModelErrors {
  const errors: SliceModelErrors = {};
  if (state.mode === 'uniform') {
    const error = fieldError(lengthValue(state.thickness, units), { positive: true });
    if (error !== undefined) {
      errors.thickness = error;
    }
  } else {
    const error = fieldError(numberValue(state.count), { min: 1, max: MAX_SLICES, integer: true });
    if (error !== undefined) {
      errors.count = error;
    }
  }
  return errors;
}

/** Kernel arguments, or null while a field is not valid. */
export function sliceModelArgs(state: SliceModelState, units: Units): SliceArgs | null {
  if (Object.keys(validateSliceModel(state, units)).length > 0) {
    return null;
  }
  const args: SliceArgs = { thicknesses: [] };
  if (state.mode === 'uniform') {
    args.thicknesses = [lengthValue(state.thickness, units) ?? 0];
    if (state.fromTop) {
      args.fromTop = true;
    }
  } else {
    args.count = numberValue(state.count) ?? 1;
  }
  if (state.boundaryVectors) {
    args.boundaryVectors = true;
  }
  if (state.heights !== null) {
    args.heights = [...state.heights];
  }
  return args;
}

/** The read outs under the slicing options, in millimetres. */
export interface SliceFacts {
  /** Number of Slices. */
  count: number;
  /** Slice Thickness: the thickest slice. */
  sliceThickness: number;
  /** Top Slice Thickness: the part slice left over when the model height
   * is not a whole number of slices; zero when it is. */
  partThickness: number;
}

/** The read outs for the slabs the kernel plans; each slab is cut off at
 * the model's own top. */
export function sliceFacts(
  slabs: ReadonlyArray<{ bottom: number; top: number }>,
  maxHeight: number,
): SliceFacts {
  const thicknesses = slabs.map((slab) => Math.max(0, Math.min(slab.top, maxHeight) - slab.bottom));
  const sliceThickness = thicknesses.reduce((a, b) => Math.max(a, b), 0);
  const part = thicknesses.find((t) => t < sliceThickness - SAME_HEIGHT);
  return { count: slabs.length, sliceThickness, partThickness: part ?? 0 };
}

/** Two heights closer than this (millimetres) are the same slice height. */
const SAME_HEIGHT = 1e-3;

/** The options that decide the slicing; changing one rebuilds the list. */
export type SlicingOptions = Pick<SliceModelState, 'mode' | 'thickness' | 'fromTop' | 'count'>;

/** Changes Model Slicing options: the Slice Heights list follows them again. */
export function changeSlicing(
  state: SliceModelState,
  patch: Partial<SlicingOptions>,
): SliceModelState {
  return { ...state, ...patch, heights: null, activeHeight: null };
}

/** The slice heights of planned slabs: every slab's top but the model's. */
export function cutHeights(slabs: ReadonlyArray<{ top: number }>): number[] {
  return slabs.slice(0, -1).map((slab) => slab.top);
}

/** The heights the list shows: the edited list, or the planned slabs'. */
export function shownHeights(
  state: SliceModelState,
  slabs: ReadonlyArray<{ top: number }>,
): number[] {
  return state.heights ?? cutHeights(slabs);
}

/** The list the form shows, lowest first: the slice heights, then the
 * model's top (entry `shown.length`, the top of the last slice). */
export function listHeights(shown: readonly number[], maxHeight: number): number[] {
  return maxHeight > 0 ? [...shown, maxHeight] : [...shown];
}

/** Whether the entry being edited is the model's top, which stays put. */
export function editingTop(state: SliceModelState, shown: readonly number[]): boolean {
  return state.activeHeight !== null && state.activeHeight >= shown.length;
}

/** Picks a list entry to edit (its height goes into the field), or none;
 * entry `shown.length` is the model's top. */
export function selectHeight(
  state: SliceModelState,
  shown: readonly number[],
  index: number | null,
  units: Units,
  maxHeight = 0,
): SliceModelState {
  const height = index === null ? undefined : listHeights(shown, maxHeight)[index];
  if (index === null || height === undefined) {
    return { ...state, activeHeight: null };
  }
  return { ...state, activeHeight: index, heightText: lengthText(height, units) };
}

/** A list edit: the new state, or why it cannot be made. */
export type HeightEdit = { ok: true; state: SliceModelState } | { ok: false; error: string };

/** `heights` with `z` in place of entry `replace` (or added), sorted. */
function placeHeight(
  state: SliceModelState,
  shown: readonly number[],
  z: number | null,
  replace: number | null,
  maxHeight: number,
  units: Units,
): HeightEdit {
  const text = strings.model.slice;
  if (z === null || !Number.isFinite(z) || z <= SAME_HEIGHT || z >= maxHeight - SAME_HEIGHT) {
    return { ok: false, error: text.badHeight(formatLength(maxHeight, units)) };
  }
  const others = shown.filter((_, k) => k !== replace);
  if (others.some((h) => Math.abs(h - z) <= SAME_HEIGHT)) {
    return { ok: false, error: text.sameHeight };
  }
  if (others.length + 1 >= MAX_SLICES) {
    return { ok: false, error: text.tooManyHeights(MAX_SLICES) };
  }
  const heights = [...others, z].sort((a, b) => a - b);
  return { ok: true, state: { ...state, heights, activeHeight: null } };
}

/**
 * Apply under the list: the typed height replaces the entry being edited,
 * or is added when none is. It must lie inside the model, apart from the
 * other heights.
 */
export function applyHeight(
  state: SliceModelState,
  shown: readonly number[],
  maxHeight: number,
  units: Units,
): HeightEdit {
  if (editingTop(state, shown)) {
    return { ok: false, error: strings.model.slice.topHeight };
  }
  const z = lengthValue(state.heightText, units);
  return placeHeight(state, shown, z, state.activeHeight, maxHeight, units);
}

/** A double click on the height bar adds a slice height there, rounded to
 * what the display unit shows. */
export function addHeightAt(
  state: SliceModelState,
  shown: readonly number[],
  z: number,
  maxHeight: number,
  units: Units,
): HeightEdit {
  const text = lengthText(z, units);
  const rounded = lengthValue(text, units) ?? z;
  const edit = placeHeight(state, shown, rounded, null, maxHeight, units);
  return edit.ok ? { ok: true, state: { ...edit.state, heightText: text } } : edit;
}

/** Delete: the entry being edited goes, joining the slices either side. */
export function deleteHeight(state: SliceModelState, shown: readonly number[]): SliceModelState {
  if (state.activeHeight === null || shown[state.activeHeight] === undefined) {
    return state;
  }
  const index = state.activeHeight;
  return { ...state, heights: shown.filter((_, k) => k !== index), activeHeight: null };
}

/** One band of the height bar, as fractions of the model height. */
export interface HeightBand {
  index: number;
  from: number;
  to: number;
}

/** The bands of the height bar, bottom up, each 0 to 1 of the height. */
export function heightBands(shown: readonly number[], maxHeight: number): HeightBand[] {
  if (!(maxHeight > 0)) {
    return [];
  }
  const edges = [0, ...shown.map((h) => Math.min(Math.max(h / maxHeight, 0), 1)), 1];
  return edges.slice(0, -1).map((from, k) => ({ index: k + 1, from, to: edges[k + 1] ?? 1 }));
}

/** The height at a point of the bar: `fromBottom` 0 at its foot, 1 at its top. */
export function heightAtBar(fromBottom: number, maxHeight: number): number {
  return Math.min(Math.max(fromBottom, 0), 1) * maxHeight;
}

/** Shift + wheel in the 3D view moves the height being edited by this much
 * per notch: millimetres, and inches in an inch job. */
export const NUDGE_MM = 0.1;
export const NUDGE_INCH = 0.005;

/** `z` rounded to what the display unit shows. */
function shownValue(z: number, units: Units): { z: number; text: string } {
  const text = lengthText(z, units);
  return { z: lengthValue(text, units) ?? z, text };
}

/** The entry being edited moved to `z`, still the one being edited. */
function moveActive(
  state: SliceModelState,
  shown: readonly number[],
  z: number,
  maxHeight: number,
  units: Units,
): HeightEdit {
  const index = state.activeHeight;
  if (index === null) {
    return { ok: true, state: { ...state, heightText: shownValue(z, units).text } };
  }
  if (editingTop(state, shown)) {
    return { ok: false, error: strings.model.slice.topHeight };
  }
  const value = shownValue(z, units);
  const edit = placeHeight(state, shown, value.z, index, maxHeight, units);
  if (!edit.ok) {
    return edit;
  }
  const heights = edit.state.heights ?? [];
  const at = heights.findIndex((h) => h === value.z);
  return {
    ok: true,
    state: { ...edit.state, activeHeight: at < 0 ? null : at, heightText: value.text },
  };
}

/**
 * An edit made in the 3D view. A double click with New height picked puts
 * the clicked height in the field; with a height picked it moves there.
 * Shift + double click inserts a height and picks it. Shift + wheel moves
 * the picked height by a notch, stopping short of the heights either side
 * and of the model's bottom and top.
 */
export function viewHeightEdit(
  state: SliceModelState,
  shown: readonly number[],
  request: ViewHeightEdit,
  maxHeight: number,
  units: Units,
): HeightEdit {
  if (request.kind === 'move') {
    return moveActive(state, shown, request.z, maxHeight, units);
  }
  if (request.kind === 'insert') {
    const value = shownValue(request.z, units);
    const edit = placeHeight(state, shown, value.z, null, maxHeight, units);
    if (!edit.ok) {
      return edit;
    }
    const at = (edit.state.heights ?? []).findIndex((h) => h === value.z);
    return {
      ok: true,
      state: { ...edit.state, activeHeight: at < 0 ? null : at, heightText: value.text },
    };
  }
  const index = state.activeHeight;
  const current = index === null ? undefined : shown[index];
  if (index === null || current === undefined || request.steps === 0) {
    return { ok: true, state };
  }
  const step = units === 'inch' ? NUDGE_INCH * 25.4 : NUDGE_MM;
  const below = shown[index - 1] ?? 0;
  const above = shown[index + 1] ?? maxHeight;
  const low = below + step;
  const high = above - step;
  const wanted = current + request.steps * step;
  const z = Math.min(Math.max(wanted, low), high);
  if (low > high || Math.abs(z - current) <= SAME_HEIGHT) {
    return { ok: true, state };
  }
  return moveActive(state, shown, z, maxHeight, units);
}

/** The slice the 3D view colours: the band of the entry being edited, from the height below it (or the
 * zero plane) to it; null with New height picked. */
export function sliceBand(
  state: SliceModelState,
  shown: readonly number[],
  maxHeight: number,
): SliceBand | null {
  const index = state.activeHeight;
  if (index === null || !(maxHeight > 0)) {
    return null;
  }
  const list = listHeights(shown, maxHeight);
  const high = list[index];
  if (high === undefined) {
    return null;
  }
  return { low: list[index - 1] ?? 0, high };
}
