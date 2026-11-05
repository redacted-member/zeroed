// Pure helpers shared by the toolpath forms: the cutting depth fields and
// the time format the toolpath list shows.

import type { Units } from '../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from './formFields';

export interface DepthState {
  startDepth: string;
  /** Depth of the cut below the start depth, as the form shows it. */
  cutDepth: string;
}

export type DepthField = 'startDepth' | 'cutDepth';
export type DepthErrors = Partial<Record<DepthField, string>>;

export interface Depths {
  startDepth: number;
  /** Depth below the surface the cut finishes at (start plus cut). */
  cutDepth: number;
  /** Always null: the pass depth is the toolpath's copy of the tool's (Edit ...). */
  passDepth: null;
}

export function initialDepths(units: Units, cutMm: number): DepthState {
  return {
    startDepth: lengthText(0, units),
    cutDepth: lengthText(cutMm, units),
  };
}

export function convertDepths(state: DepthState, from: Units, to: Units): DepthState {
  return convertLengthFields(state, ['startDepth', 'cutDepth'], from, to);
}

export function validateDepths(state: DepthState, units: Units): DepthErrors {
  const errors: DepthErrors = {};
  const start = lengthValue(state.startDepth, units);
  const cut = lengthValue(state.cutDepth, units);
  const startError = fieldError(start, { min: 0 });
  const cutError = fieldError(cut, { positive: true });
  if (startError !== undefined) {
    errors.startDepth = startError;
  }
  if (cutError !== undefined) {
    errors.cutDepth = cutError;
  }
  return errors;
}

/** Depths in millimetres, or null while a field is invalid. */
export function depthValues(state: DepthState, units: Units): Depths | null {
  if (Object.keys(validateDepths(state, units)).length > 0) {
    return null;
  }
  const startDepth = lengthValue(state.startDepth, units) ?? 0;
  return {
    startDepth,
    cutDepth: startDepth + (lengthValue(state.cutDepth, units) ?? 0),
    passDepth: null,
  };
}

/**
 * The Cut Depth field's text for a stored toolpath: the form measures the
 * cut from the start depth, the kernel from the surface.
 */
export function cutDepthText(startDepth: number, cutDepth: number, units: Units): string {
  return lengthText(Math.max(0, cutDepth - startDepth), units);
}

/** Hours, minutes and seconds, as the toolpath list shows machining time. */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const two = (n: number) => String(n).padStart(2, '0');
  return `${h}:${two(m)}:${two(s)}`;
}
