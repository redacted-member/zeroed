// Pure state for the Trace Bitmap form: the trace settings for the bitmap
// selected in the view, colour picks and the trace area.

import type { TraceFit, TraceParams } from '../../bridge/api';
import type { Bbox, Path, Point } from '../../bridge/document';
import { fieldError, numberValue } from '../../lib/formFields';
import { rectPath } from '../../lib/geometry';

export interface TraceBitmapState {
  threshold: string;
  /** Smallest kept area, in pixels. */
  noise: string;
  fit: TraceFit;
  /** Allowed deviation from the pixel outline, in pixels. */
  smoothness: string;
}

export type TraceField = 'threshold' | 'noise' | 'smoothness';
export type TraceErrors = Partial<Record<TraceField, string>>;

export function initialTraceState(): TraceBitmapState {
  return { threshold: '128', noise: '4', fit: 'curves', smoothness: '1' };
}

export function traceErrors(state: TraceBitmapState): TraceErrors {
  const errors: TraceErrors = {
    threshold: fieldError(numberValue(state.threshold), { min: 0, max: 255, integer: true }),
    noise: fieldError(numberValue(state.noise), { min: 0 }),
    smoothness: fieldError(numberValue(state.smoothness), { positive: true }),
  };
  for (const key of Object.keys(errors) as TraceField[]) {
    if (errors[key] === undefined) {
      delete errors[key];
    }
  }
  return errors;
}

/** The trace settings the form holds, or null while one is invalid. The
 * dark areas below the threshold are traced. */
export function traceParamsOf(state: TraceBitmapState): TraceParams | null {
  const threshold = numberValue(state.threshold);
  const noise = numberValue(state.noise);
  const smoothness = numberValue(state.smoothness);
  if (
    Object.keys(traceErrors(state)).length > 0 ||
    threshold === null ||
    noise === null ||
    smoothness === null
  ) {
    return null;
  }
  return { threshold, invert: false, noise, cornerFit: state.fit, smoothness };
}

/** The file name part of a path, for a form to show. */
export function shownName(path: string | null): string | null {
  if (path === null) {
    return null;
  }
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

/**
 * The bitmap the form traces: the one bitmap among the selected entities,
 * or null when none or several are selected.
 */
export function selectedBitmap(
  entities: ReadonlyArray<{ id: number; kind: string }>,
  selection: readonly number[],
): number | null {
  const bitmaps = entities.filter((e) => e.kind === 'bitmap' && selection.includes(e.id));
  return bitmaps.length === 1 ? (bitmaps[0]?.id ?? null) : null;
}

/** Bitmap Fading as a percentage, from a bitmap's opacity. */
export function fadeOf(opacity: number): number {
  return Math.round((1 - Math.min(1, Math.max(0, opacity))) * 100);
}

/** The opacity a Bitmap Fading percentage gives. */
export function opacityOf(fade: number): number {
  return 1 - Math.min(100, Math.max(0, fade)) / 100;
}

/**
 * Colour tracing: how many colours the image is reduced to, the trace
 * colour picked from the list or the picture, and the colours linked to it
 * so they are traced as one area.
 */
export interface ColourTraceState {
  count: number;
  trace: number | null;
  linked: number[];
}

export const MIN_COLOURS = 2;
export const MAX_COLOURS = 16;

export function initialColourState(): ColourTraceState {
  return { count: MAX_COLOURS, trace: null, linked: [] };
}

/** A new colour count renumbers the palette, so the picks start over. */
export function withColourCount(state: ColourTraceState, count: number): ColourTraceState {
  const clamped = Math.min(MAX_COLOURS, Math.max(MIN_COLOURS, Math.round(count)));
  return clamped === state.count ? state : { count: clamped, trace: null, linked: [] };
}

export function pickColour(state: ColourTraceState, index: number | null): ColourTraceState {
  return { ...state, trace: index };
}

export function toggleLinked(state: ColourTraceState, index: number): ColourTraceState {
  const linked = state.linked.includes(index)
    ? state.linked.filter((i) => i !== index)
    : [...state.linked, index].sort((a, b) => a - b);
  return { ...state, linked };
}

export function resetLinked(state: ColourTraceState): ColourTraceState {
  return { ...state, linked: [] };
}

/** The palette indices traced together, or an empty list before a pick. */
export function tracedColours(state: ColourTraceState): number[] {
  const all = new Set(state.linked);
  if (state.trace !== null) {
    all.add(state.trace);
  }
  return [...all].sort((a, b) => a - b);
}

/** `#rrggbb` for a palette colour. */
export function colourHex(rgb: readonly [number, number, number]): string {
  return `#${rgb.map((v) => Math.round(v).toString(16).padStart(2, '0')).join('')}`;
}

/** The trace area a drag spans, or null for a click (smaller than `minSize` mm). */
export function regionFromDrag(from: Point, to: Point, minSize: number): Bbox | null {
  const box: Bbox = [
    Math.min(from.x, to.x),
    Math.min(from.y, to.y),
    Math.max(from.x, to.x),
    Math.max(from.y, to.y),
  ];
  return box[2] - box[0] < minSize || box[3] - box[1] < minSize ? null : box;
}

/** The trace area outline, drawn dashed while the form is open. */
export function regionPaths(region: Bbox | null): Path[] {
  return region === null
    ? []
    : [rectPath(region[0], region[1], region[2] - region[0], region[3] - region[1])];
}

/**
 * RGBA pixels of the picture in its reduced colours: every pixel of a traced
 * colour shows the trace colour, the rest their own palette colour, and
 * transparent pixels (index 255) stay clear.
 */
export function reducedPixels(
  indices: Uint8Array,
  palette: ReadonlyArray<readonly [number, number, number]>,
  state: ColourTraceState,
): Uint8ClampedArray<ArrayBuffer> {
  const traced = new Set(tracedColours(state));
  const traceRgb = state.trace !== null ? palette[state.trace] : undefined;
  const out = new Uint8ClampedArray(new ArrayBuffer(indices.length * 4));
  indices.forEach((index, i) => {
    const rgb = traced.has(index) && traceRgb !== undefined ? traceRgb : palette[index];
    if (rgb === undefined) {
      return;
    }
    out[i * 4] = rgb[0];
    out[i * 4 + 1] = rgb[1];
    out[i * 4 + 2] = rgb[2];
    out[i * 4 + 3] = 255;
  });
  return out;
}

/** Smoothness in pixels at the Loose and the Tight end of Corner Fit. */
export const LOOSE_PX = 4;
export const TIGHT_PX = 0.25;

/** The smoothness a Corner Fit slider position (0 Loose to 100 Tight)
 * asks for, on a log scale between the two ends. */
export function smoothnessFromCornerFit(position: number): number {
  const t = Math.min(100, Math.max(0, position)) / 100;
  return Math.round(LOOSE_PX * Math.pow(TIGHT_PX / LOOSE_PX, t) * 1000) / 1000;
}

/** Where the Corner Fit slider sits for a smoothness. */
export function cornerFitFromSmoothness(smoothness: number | null): number {
  if (smoothness === null || !(smoothness > 0)) {
    return 50;
  }
  const t = Math.log(smoothness / LOOSE_PX) / Math.log(TIGHT_PX / LOOSE_PX);
  return Math.round(Math.min(1, Math.max(0, t)) * 100);
}
