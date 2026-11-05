// Pure state for Edit ... on a toolpath form's tool row: the cutting
// parameters, feeds and speeds and tool number of this toolpath's own copy
// of the tool. The database tool is left as it is.

import { toolDiameter, type Tool } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { fieldError, lengthText, lengthValue, numberValue } from '../../lib/formFields';

export interface ToolEditState {
  passDepth: string;
  stepover: string;
  stepoverPercent: string;
  spindleSpeed: string;
  feedRate: string;
  plungeRate: string;
  number: string;
}

export type ToolEditField = keyof ToolEditState;
export type ToolEditErrors = Partial<Record<ToolEditField, string>>;

function round(value: number, places: number): string {
  return String(Number(value.toFixed(places)));
}

export function toolEditState(tool: Tool, units: Units): ToolEditState {
  return {
    passDepth: lengthText(tool.passDepth, units),
    stepover: lengthText(tool.stepover, units),
    stepoverPercent: round(tool.stepoverPercent, 2),
    spindleSpeed: round(tool.spindleSpeed, 0),
    feedRate: round(tool.feedRate, 3),
    plungeRate: round(tool.plungeRate, 3),
    number: String(tool.number),
  };
}

/** The stepover distance changed: the percentage follows the tool diameter. */
export function setStepover(
  state: ToolEditState,
  text: string,
  tool: Tool,
  units: Units,
): ToolEditState {
  const value = lengthValue(text, units);
  const diameter = toolDiameter(tool);
  return {
    ...state,
    stepover: text,
    stepoverPercent:
      value === null || diameter <= 0 ? state.stepoverPercent : round((value / diameter) * 100, 2),
  };
}

/** The stepover percentage changed: the distance follows the tool diameter. */
export function setStepoverPercent(
  state: ToolEditState,
  text: string,
  tool: Tool,
  units: Units,
): ToolEditState {
  const percent = numberValue(text);
  const diameter = toolDiameter(tool);
  return {
    ...state,
    stepoverPercent: text,
    stepover:
      percent === null || diameter <= 0
        ? state.stepover
        : lengthText((diameter * percent) / 100, units),
  };
}

export function validateToolEdit(state: ToolEditState, units: Units): ToolEditErrors {
  const errors: ToolEditErrors = {};
  const checks: Array<[ToolEditField, string | undefined]> = [
    ['passDepth', fieldError(lengthValue(state.passDepth, units), { positive: true })],
    ['stepover', fieldError(lengthValue(state.stepover, units), { positive: true })],
    [
      'stepoverPercent',
      fieldError(numberValue(state.stepoverPercent), { positive: true, max: 100 }),
    ],
    ['spindleSpeed', fieldError(numberValue(state.spindleSpeed), { min: 0 })],
    ['feedRate', fieldError(numberValue(state.feedRate), { positive: true })],
    ['plungeRate', fieldError(numberValue(state.plungeRate), { positive: true })],
    ['number', fieldError(numberValue(state.number), { min: 0, integer: true })],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** The toolpath's edited copy of the tool, or null while a field is invalid. */
export function editedTool(tool: Tool, state: ToolEditState, units: Units): Tool | null {
  if (Object.keys(validateToolEdit(state, units)).length > 0) {
    return null;
  }
  return {
    ...tool,
    passDepth: lengthValue(state.passDepth, units) ?? tool.passDepth,
    stepover: lengthValue(state.stepover, units) ?? tool.stepover,
    stepoverPercent: numberValue(state.stepoverPercent) ?? tool.stepoverPercent,
    spindleSpeed: numberValue(state.spindleSpeed) ?? tool.spindleSpeed,
    feedRate: numberValue(state.feedRate) ?? tool.feedRate,
    plungeRate: numberValue(state.plungeRate) ?? tool.plungeRate,
    number: numberValue(state.number) ?? tool.number,
  };
}

/**
 * A toolpath saved when its form could override the tool's pass depth or
 * stepover: the overrides become the toolpath's copy of the tool, and the
 * params no longer carry them.
 */
export function foldToolOverrides<P>(tool: Tool, params: P): { tool: Tool; params: P } {
  if (typeof params !== 'object' || params === null) {
    return { tool, params };
  }
  const record = params as Record<string, unknown>;
  const passDepth = record.passDepth;
  const stepover = record.stepover;
  if (typeof passDepth !== 'number' && typeof stepover !== 'number') {
    return { tool, params };
  }
  return {
    tool: {
      ...tool,
      ...(typeof passDepth === 'number' ? { passDepth } : {}),
      ...(typeof stepover === 'number' ? { stepover } : {}),
    },
    params: {
      ...record,
      ...(typeof passDepth === 'number' ? { passDepth: null } : {}),
      ...(typeof stepover === 'number' ? { stepover: null } : {}),
    } as P,
  };
}

/**
 * A V-Groove saved with its own Max Pass Depth: the depth becomes the pass
 * depth of the toolpath's copy of the tool, as the form now cuts in the
 * tool's passes.
 */
export function foldMaxPassDepth<P>(tool: Tool, params: P): { tool: Tool; params: P } {
  if (typeof params !== 'object' || params === null) {
    return { tool, params };
  }
  const record = params as Record<string, unknown>;
  const pass = record.maxPassDepth;
  if (typeof pass !== 'number' || !(pass > 0)) {
    return { tool, params };
  }
  return { tool: { ...tool, passDepth: pass }, params: { ...record, maxPassDepth: null } as P };
}
