// Pure state for the V-Groove Toolpath form. The V is cut in passes no
// deeper than the tool's own pass depth (Edit ... changes it); Use
// Clearance Tools holds the ordered clearance tools, each with its options.

import type { VGrooveParams } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import type { ClearanceErrors } from '../../lib/clearanceFields';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import {
  convertEntries,
  entriesFrom,
  kernelEntries,
  sharedOptions,
  validateEntries,
  type ClearanceEntry,
} from '../ClearanceToolsField/model';

export interface VGrooveFormState {
  name: string;
  startDepth: string;
  useFlatDepth: boolean;
  flatDepth: string;
  /** Use Clearance Tools is ticked. */
  useClearance: boolean;
  /** The clearance tools in cutting order, each with its own options. */
  clearanceTools: ClearanceEntry[];
  selectionOrder: boolean;
  vectorStart: boolean;
}

export type VGrooveErrors = ClearanceErrors & Partial<Record<'startDepth' | 'flatDepth', string>>;

export function initialVGrooveState(units: Units): VGrooveFormState {
  const inch = units === 'inch';
  return {
    name: '',
    startDepth: lengthText(0, units),
    useFlatDepth: false,
    flatDepth: lengthText(inch ? 6.35 : 6, units),
    useClearance: false,
    clearanceTools: [],
    selectionOrder: false,
    vectorStart: false,
  };
}

export function convertVGrooveUnits(
  state: VGrooveFormState,
  from: Units,
  to: Units,
): VGrooveFormState {
  return {
    ...convertLengthFields(state, ['startDepth', 'flatDepth'], from, to),
    clearanceTools: convertEntries(state.clearanceTools, from, to),
  };
}

export function validateVGroove(state: VGrooveFormState, units: Units): VGrooveErrors {
  const invalid = state.useClearance ? validateEntries(state.clearanceTools, units) : null;
  const errors: VGrooveErrors = invalid === null ? {} : { ...invalid.errors };
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

export function vgrooveParams(state: VGrooveFormState, units: Units): VGrooveParams | null {
  if (Object.keys(validateVGroove(state, units)).length > 0) {
    return null;
  }
  const tools = state.useClearance ? state.clearanceTools : [];
  return {
    startDepth: lengthValue(state.startDepth, units) ?? 0,
    flatDepth: state.useFlatDepth ? lengthValue(state.flatDepth, units) : null,
    // The tool's pass depth sets the passes.
    maxPassDepth: null,
    clearanceTool: null,
    clearanceOptions: sharedOptions(state.clearanceTools, units),
    clearanceTools: kernelEntries(tools, units),
    selectionOrder: state.selectionOrder,
    vectorStart: state.vectorStart,
  };
}

export function vgrooveStateFrom(
  params: VGrooveParams,
  name: string,
  units: Units,
): VGrooveFormState {
  const base = initialVGrooveState(units);
  const list = params.clearanceTools ?? [];
  const legacy = params.clearanceTool ?? null;
  const saved =
    list.length === 0 && legacy !== null ? [{ tool: legacy, cornerSharpen: false }] : list;
  const entries = entriesFrom(saved, params.clearanceOptions, units);
  return {
    name,
    startDepth: lengthText(params.startDepth, units),
    useFlatDepth: params.flatDepth !== null,
    flatDepth: params.flatDepth === null ? base.flatDepth : lengthText(params.flatDepth, units),
    useClearance: entries.length > 0,
    clearanceTools: entries,
    selectionOrder: params.selectionOrder ?? false,
    vectorStart: params.vectorStart ?? false,
  };
}
