// Pure state for the Drilling Toolpath form.

import type { DrillParams } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import { cutDepthText } from '../../lib/toolpathFields';

export interface DrillFormState {
  name: string;
  selectionOrder: boolean;
  startDepth: string;
  cutDepth: string;
  peck: boolean;
  fullRetract: boolean;
  /** Retract Gap (R), above the start depth or the previous pass. */
  retractGap: string;
  /** Pause at the bottom of each pass. */
  useDwell: boolean;
  dwell: string;
}

export type DrillField = 'startDepth' | 'cutDepth' | 'retractGap' | 'dwell';
export type DrillErrors = Partial<Record<DrillField, string>>;

/** Retract Gap of a toolpath saved before the field existed: the fixed
 * gap it was cut with, in millimetres. A new form starts at 0, as the
 * reference form does. */
const OLDER_RETRACT_GAP = 1;

export function initialDrillState(units: Units): DrillFormState {
  const inch = units === 'inch';
  return {
    selectionOrder: false,
    name: '',
    startDepth: lengthText(0, units),
    cutDepth: lengthText(inch ? 12.7 : 10, units),
    peck: false,
    fullRetract: true,
    retractGap: lengthText(0, units),
    useDwell: false,
    dwell: '1',
  };
}

export function convertDrillUnits(state: DrillFormState, from: Units, to: Units): DrillFormState {
  return convertLengthFields(state, ['startDepth', 'cutDepth', 'retractGap'], from, to);
}

export function validateDrill(state: DrillFormState, units: Units): DrillErrors {
  const errors: DrillErrors = {};
  const start = lengthValue(state.startDepth, units);
  const cut = lengthValue(state.cutDepth, units);
  const checks: Array<[DrillField, string | undefined]> = [
    ['startDepth', fieldError(start, { min: 0 })],
    ['cutDepth', fieldError(cut, { positive: true })],
  ];
  if (state.useDwell) {
    checks.push(['dwell', fieldError(numberValue(state.dwell), { min: 0, max: 60 })]);
  }
  if (state.peck) {
    checks.push(['retractGap', fieldError(lengthValue(state.retractGap, units), { min: 0 })]);
  }
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/**
 * Kernel parameters, or null while a field is invalid. Pecks go the tool's
 * pass depth (`toolPassDepth`, millimetres) at a time.
 */
export function drillParams(
  state: DrillFormState,
  units: Units,
  toolPassDepth: number,
): DrillParams | null {
  if (Object.keys(validateDrill(state, units)).length > 0) {
    return null;
  }
  const startDepth = lengthValue(state.startDepth, units) ?? 0;
  return {
    selectionOrder: state.selectionOrder,
    startDepth,
    // The form measures the cut from the start depth.
    cutDepth: startDepth + (lengthValue(state.cutDepth, units) ?? 0),
    peckDepth: state.peck ? toolPassDepth : 0,
    fullRetract: state.fullRetract,
    retractGap: lengthValue(state.retractGap, units) ?? 0,
    dwell: state.useDwell ? (numberValue(state.dwell) ?? 0) : 0,
  };
}

/** Form state for an existing toolpath's parameters. */
export function drillStateFrom(params: DrillParams, name: string, units: Units): DrillFormState {
  return {
    name,
    selectionOrder: params.selectionOrder ?? false,
    startDepth: lengthText(params.startDepth, units),
    cutDepth: cutDepthText(params.startDepth, params.cutDepth, units),
    peck: params.peckDepth > 0,
    fullRetract: params.fullRetract,
    retractGap:
      params.retractGap === undefined
        ? lengthText(OLDER_RETRACT_GAP, units)
        : lengthText(params.retractGap, units),
    useDwell: params.dwell > 0,
    dwell: params.dwell > 0 ? String(params.dwell) : initialDrillState(units).dwell,
  };
}
