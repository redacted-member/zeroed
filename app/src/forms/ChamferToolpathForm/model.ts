// Pure state for the Chamfer Toolpath form.

import type {
  ChamferParams,
  ChamferSlope,
  CutDirection,
  ProfileSide,
  Tool,
} from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import { strings } from '../../lib/formStrings';

/** Whether the bevel's width or its depth is given; the other follows. */
export type ChamferBy = 'width' | 'depth';

export interface ChamferFormState {
  name: string;
  selectionOrder: boolean;
  by: ChamferBy;
  width: string;
  depth: string;
  side: ProfileSide;
  startDepth: string;
  direction: CutDirection;
  overcut: string;
  slope: ChamferSlope;
  angle: string;
}

export type ChamferField = 'width' | 'depth' | 'startDepth' | 'overcut' | 'angle';
export type ChamferErrors = Partial<Record<ChamferField, string>>;

const DEFAULT_WIDTH_MM = 3;

export function initialChamferState(units: Units): ChamferFormState {
  const size = units === 'inch' ? 3.175 : DEFAULT_WIDTH_MM;
  return {
    selectionOrder: false,
    name: '',
    by: 'width',
    width: lengthText(size, units),
    depth: lengthText(size, units),
    side: 'inside',
    startDepth: lengthText(0, units),
    direction: 'climb',
    overcut: lengthText(0, units),
    slope: 'down',
    angle: '45',
  };
}

export function convertChamferUnits(
  state: ChamferFormState,
  from: Units,
  to: Units,
): ChamferFormState {
  return convertLengthFields(state, ['width', 'depth', 'startDepth', 'overcut'], from, to);
}

export function validateChamfer(state: ChamferFormState, units: Units): ChamferErrors {
  const errors: ChamferErrors = {};
  const checks: Array<[ChamferField, string | undefined]> = [
    ['startDepth', fieldError(lengthValue(state.startDepth, units), { min: 0 })],
    state.by === 'width'
      ? ['width', fieldError(lengthValue(state.width, units), { positive: true })]
      : ['depth', fieldError(lengthValue(state.depth, units), { positive: true })],
    ['overcut', fieldError(lengthValue(state.overcut, units), { min: 0 })],
    ['angle', fieldError(numberValue(state.angle), { min: 1, max: 89 })],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** True for the tools whose cone can cut a bevel. */
export function isVShaped(tool: Tool): boolean {
  const kind = tool.toolType.kind;
  return kind === 'vBit' || kind === 'engravingTool';
}

/** A problem with the picked tool, shown beside the tool picker. */
export function chamferToolWarning(tool: Tool | null): string | undefined {
  return tool !== null && !isVShaped(tool) && tool.toolType.kind !== 'ballNose'
    ? strings.toolpaths.chamfer.notV
    : undefined;
}

/** Bevel angle from the vertical in degrees: the tool's half angle for a V
 * shaped tool, the form's angle otherwise. */
export function chamferAngle(state: ChamferFormState, tool: Tool | null): number | null {
  if (tool !== null && isVShaped(tool)) {
    const included = tool.toolType.includedAngle;
    return typeof included === 'number' ? included / 2 : null;
  }
  return numberValue(state.angle);
}

/** The deepest the chamfer cuts below the surface in millimetres, as the
 * form shows it, or null while it cannot be worked out. A ball nose goes a
 * little deeper than this, by how far its centre sits off the face. */
export function chamferMaxDepth(
  state: ChamferFormState,
  units: Units,
  tool: Tool | null,
): number | null {
  const angle = chamferAngle(state, tool);
  const start = lengthValue(state.startDepth, units);
  const overcut = lengthValue(state.overcut, units) ?? 0;
  if (tool === null || angle === null || start === null || angle <= 0 || angle >= 90) {
    return null;
  }
  const tan = Math.tan((angle * Math.PI) / 180);
  const depth =
    state.by === 'depth'
      ? lengthValue(state.depth, units)
      : (() => {
          const width = lengthValue(state.width, units);
          return width === null ? null : width / tan;
        })();
  if (depth === null) {
    return null;
  }
  const oneSided = state.side !== 'on';
  return start + depth + (oneSided ? overcut / tan : 0);
}

/**
 * The Width (W) and Cut Depth (C) texts the form shows: the one typed last
 * drives, the other follows from the bevel angle (blank while the angle is
 * unknown, such as before a tool is picked).
 */
export function chamferSizes(
  state: ChamferFormState,
  units: Units,
  tool: Tool | null,
): { width: string; depth: string } {
  const angle = chamferAngle(state, tool);
  const tan = angle !== null && angle > 0 && angle < 90 ? Math.tan((angle * Math.PI) / 180) : null;
  if (state.by === 'width') {
    const width = lengthValue(state.width, units);
    return {
      width: state.width,
      depth: tan === null || width === null ? state.depth : lengthText(width / tan, units),
    };
  }
  const depth = lengthValue(state.depth, units);
  return {
    width: tan === null || depth === null ? state.width : lengthText(depth * tan, units),
    depth: state.depth,
  };
}

/** Typing into Width or Cut Depth makes that field drive the chamfer. */
export function editChamferSize(
  state: ChamferFormState,
  field: ChamferBy,
  text: string,
  units: Units,
  tool: Tool | null,
): ChamferFormState {
  const shown = chamferSizes(state, units, tool);
  return { ...state, ...shown, by: field, [field]: text };
}

export function chamferParams(state: ChamferFormState, units: Units): ChamferParams | null {
  if (Object.keys(validateChamfer(state, units)).length > 0) {
    return null;
  }
  const width = lengthValue(state.width, units);
  return {
    selectionOrder: state.selectionOrder,
    width: width !== null && width > 0 ? width : DEFAULT_WIDTH_MM,
    depth: state.by === 'depth' ? lengthValue(state.depth, units) : null,
    side: state.side,
    startDepth: lengthValue(state.startDepth, units) ?? 0,
    direction: state.direction,
    overcut: lengthValue(state.overcut, units) ?? 0,
    slope: state.slope,
    angle: numberValue(state.angle) ?? 45,
  };
}

export function chamferStateFrom(
  params: ChamferParams,
  name: string,
  units: Units,
): ChamferFormState {
  const base = initialChamferState(units);
  return {
    name,
    selectionOrder: params.selectionOrder ?? false,
    by: params.depth === null ? 'width' : 'depth',
    width: lengthText(params.width, units),
    depth: params.depth === null ? base.depth : lengthText(params.depth, units),
    side: params.side,
    startDepth: lengthText(params.startDepth, units),
    direction: params.direction ?? 'climb',
    overcut: lengthText(params.overcut ?? 0, units),
    slope: params.slope ?? 'up',
    angle: String(params.angle ?? 45),
  };
}
