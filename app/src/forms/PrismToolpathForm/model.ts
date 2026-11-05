// Pure state for the Prism Carve Toolpath form.

import type { CutDirection, PrismDirection, PrismParams, Tool } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { isVShaped } from '../ChamferToolpathForm/model';

export interface PrismFormState {
  name: string;
  selectionOrder: boolean;
  direction: PrismDirection;
  limitDepth: boolean;
  maxDepth: string;
  startDepth: string;
  cutDirection: CutDirection;
}

export type PrismField = 'maxDepth' | 'startDepth';
export type PrismErrors = Partial<Record<PrismField, string>>;

const DEFAULT_MAX_DEPTH_MM = 25;

export function initialPrismState(units: Units): PrismFormState {
  return {
    selectionOrder: false,
    name: '',
    direction: 'outside',
    limitDepth: true,
    maxDepth: lengthText(units === 'inch' ? 25.4 : DEFAULT_MAX_DEPTH_MM, units),
    startDepth: lengthText(0, units),
    cutDirection: 'conventional',
  };
}

export function convertPrismUnits(state: PrismFormState, from: Units, to: Units): PrismFormState {
  return convertLengthFields(state, ['maxDepth', 'startDepth'], from, to);
}

export function validatePrism(state: PrismFormState, units: Units): PrismErrors {
  const errors: PrismErrors = {};
  const start = fieldError(lengthValue(state.startDepth, units), { min: 0 });
  if (start !== undefined) {
    errors.startDepth = start;
  }
  if (state.limitDepth) {
    const max = fieldError(lengthValue(state.maxDepth, units), { positive: true });
    if (max !== undefined) {
      errors.maxDepth = max;
    }
  }
  return errors;
}

/** A problem with the picked tool, shown beside the tool picker. */
export function prismToolWarning(tool: Tool | null): string | undefined {
  return tool !== null && !isVShaped(tool) ? strings.toolpaths.prism.notV : undefined;
}

/** One line on what the chosen direction does. */
export function prismDirectionHint(direction: PrismDirection): string {
  const text = strings.toolpaths.prism;
  return direction === 'outside' ? text.outsideHint : text.insideHint;
}

export function prismParams(state: PrismFormState, units: Units): PrismParams | null {
  if (Object.keys(validatePrism(state, units)).length > 0) {
    return null;
  }
  return {
    selectionOrder: state.selectionOrder,
    direction: state.direction,
    maxDepth: state.limitDepth ? lengthValue(state.maxDepth, units) : null,
    startDepth: lengthValue(state.startDepth, units) ?? 0,
    cutDirection: state.cutDirection,
  };
}

export function prismStateFrom(params: PrismParams, name: string, units: Units): PrismFormState {
  return {
    name,
    selectionOrder: params.selectionOrder ?? false,
    direction: params.direction,
    // Toolpaths saved without a depth carve full prisms: the field stays empty.
    limitDepth: params.maxDepth !== null,
    maxDepth: params.maxDepth === null ? '' : lengthText(params.maxDepth, units),
    startDepth: lengthText(params.startDepth, units),
    cutDirection: params.cutDirection ?? 'climb',
  };
}

/** Typing the Prism Depth; an empty field carves full prisms. */
export function withPrismDepth(state: PrismFormState, text: string): PrismFormState {
  return { ...state, maxDepth: text, limitDepth: text.trim() !== '' };
}

/** The form with the depth for full prisms (millimetres) filled in. */
export function withFullDepth(
  state: PrismFormState,
  depthMm: number,
  units: Units,
): PrismFormState {
  return { ...state, limitDepth: true, maxDepth: lengthText(depthMm, units) };
}
