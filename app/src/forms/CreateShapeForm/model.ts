// Pure state for the five Create Shape tools (Flat, Round, Angled, Smooth
// and Custom): which settings each shows, validation, the kernel payload,
// editing a shape the tools made before, and the Total Height box.

import type {
  Combine,
  ComponentSummary,
  ShapeArgs,
  ShapeParams,
  ShapeProfile,
} from '../../bridge/model';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import { dynamicState, fadeProp, tiltProp, type DynamicState } from '../DynamicFields/model';

/** One of the shape tools of the Modeling tab. */
export type ShapeTool = 'flat' | 'round' | 'angled' | 'smooth' | 'custom';

export const SHAPE_TOOLS: readonly ShapeTool[] = ['flat', 'round', 'angled', 'smooth', 'custom'];

const PROFILE: Record<ShapeTool, ShapeProfile> = {
  flat: 'flat',
  round: 'round',
  angled: 'angular',
  smooth: 'smooth',
  custom: 'custom',
};

export function profileOf(tool: ShapeTool): ShapeProfile {
  return PROFILE[tool];
}

export function toolOf(profile: ShapeProfile): ShapeTool {
  return SHAPE_TOOLS.find((tool) => PROFILE[tool] === profile) ?? 'round';
}

export interface ShapeFormState {
  name: string;
  combine: Combine;
  /** Round and angled: degrees from flat at the edge. */
  angle: string;
  limitOn: boolean;
  /** Display unit. */
  limit: string;
  sharpCorners: boolean;
  /** Smooth and custom. */
  blendInner: boolean;
  scaleOn: boolean;
  /** Display unit. */
  scaleHeight: string;
  /** Display unit. */
  baseHeight: string;
  /** Smooth: percent, 0 to 100. */
  smoothTop: string;
  /** Smooth: percent, 0 to 100. */
  smoothBottom: string;
  dynamic: DynamicState;
}

export type ShapeField =
  'angle' | 'limit' | 'scaleHeight' | 'baseHeight' | 'smoothTop' | 'smoothBottom';
export type ShapeErrors = Partial<Record<ShapeField, string>>;

/** Which settings a tool shows, in the order its form lists them. */
export interface ShapeFields {
  angle: boolean;
  smoothing: boolean;
  limit: boolean;
  sharpCorners: boolean;
  blendInner: boolean;
  scale: boolean;
  totalHeight: boolean;
}

export function fieldsOf(tool: ShapeTool): ShapeFields {
  const shaped = tool !== 'flat';
  return {
    angle: tool === 'round' || tool === 'angled',
    smoothing: tool === 'smooth',
    limit: shaped,
    sharpCorners: shaped,
    blendInner: tool === 'smooth' || tool === 'custom',
    scale: shaped,
    totalHeight: shaped,
  };
}

/** The angle a tool opens at: upright edges for Round, a 45 degree
 * chamfer for Angled (upright sides there need Limit to Height). */
export function defaultAngle(tool: ShapeTool): string {
  return tool === 'angled' ? '45' : '90';
}

export function initialShapeState(units: Units, tool: ShapeTool = 'round'): ShapeFormState {
  const five = units === 'inch' ? 6.35 : 5;
  return {
    name: '',
    combine: 'add',
    angle: defaultAngle(tool),
    limitOn: false,
    limit: lengthText(five, units),
    sharpCorners: false,
    blendInner: false,
    scaleOn: false,
    scaleHeight: lengthText(five, units),
    baseHeight: lengthText(0, units),
    smoothTop: '50',
    smoothBottom: '50',
    dynamic: dynamicState(),
  };
}

function percentText(fraction: number): string {
  return String(Math.round(fraction * 1000) / 10);
}

/** The form as a shape component was last applied, for editing it. */
export function stateFromComponent(component: ComponentSummary, units: Units): ShapeFormState {
  const initial = initialShapeState(units);
  const params = component.shape?.params;
  if (params === undefined) {
    return { ...initial, name: component.name, combine: component.combine };
  }
  return {
    name: component.name,
    combine: component.combine,
    angle: String(params.angle),
    limitOn: params.limit,
    limit: params.limit ? lengthText(params.height, units) : initial.limit,
    sharpCorners: params.sharpCorners,
    blendInner: params.blendInner ?? false,
    scaleOn: params.scale ?? false,
    scaleHeight: lengthText(params.scaleHeight ?? 5, units),
    baseHeight: lengthText(component.baseHeight ?? 0, units),
    smoothTop: percentText(params.smoothTop),
    smoothBottom: percentText(params.smoothBottom),
    dynamic: dynamicState(component.fade, component.tilt),
  };
}

/** The shape component a tool opens on: the one component picked, when
 * this tool made it. */
export function editableShape(
  components: readonly ComponentSummary[],
  picked: readonly number[],
  tool: ShapeTool,
): ComponentSummary | null {
  if (picked.length !== 1) {
    return null;
  }
  const component = components.find((c) => c.id === picked[0]);
  const params = component?.shape?.params;
  return component !== undefined && params !== undefined && params.profile === PROFILE[tool]
    ? component
    : null;
}

export function convertShapeUnits(state: ShapeFormState, from: Units, to: Units): ShapeFormState {
  return convertLengthFields(state, ['limit', 'scaleHeight', 'baseHeight'], from, to);
}

export function validateShape(state: ShapeFormState, tool: ShapeTool, units: Units): ShapeErrors {
  const fields = fieldsOf(tool);
  const errors: ShapeErrors = {};
  const checks: Array<[ShapeField, string | undefined]> = [
    ['baseHeight', fieldError(lengthValue(state.baseHeight, units))],
  ];
  if (fields.angle) {
    const angle = numberValue(state.angle);
    const upright = tool === 'angled' && !state.limitOn && angle !== null && angle >= 90;
    checks.push([
      'angle',
      upright
        ? strings.model.createShape.uprightNeedsLimit
        : fieldError(angle, { positive: true, max: 90 }),
    ]);
  }
  if (fields.smoothing) {
    checks.push(['smoothTop', fieldError(numberValue(state.smoothTop), { min: 0, max: 100 })]);
    checks.push([
      'smoothBottom',
      fieldError(numberValue(state.smoothBottom), { min: 0, max: 100 }),
    ]);
  }
  if (fields.limit && state.limitOn) {
    checks.push(['limit', fieldError(lengthValue(state.limit, units), { positive: true })]);
  }
  if (fields.scale && state.scaleOn) {
    checks.push([
      'scaleHeight',
      fieldError(lengthValue(state.scaleHeight, units), { positive: true }),
    ]);
  }
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** A 0 to 100 percent field as the kernel's 0 to 1 fraction. */
function fraction(percent: string): number {
  return Math.min(1, Math.max(0, (numberValue(percent) ?? 50) / 100));
}

export function shapeParams(
  state: ShapeFormState,
  tool: ShapeTool,
  units: Units,
): ShapeParams | null {
  if (Object.keys(validateShape(state, tool, units)).length > 0) {
    return null;
  }
  const fields = fieldsOf(tool);
  return {
    profile: PROFILE[tool],
    angle: fields.angle ? (numberValue(state.angle) ?? 90) : 90,
    height: fields.limit ? (lengthValue(state.limit, units) ?? 5) : 0,
    limit: fields.limit && state.limitOn,
    baseHeight: 0,
    smoothTop: fraction(state.smoothTop),
    smoothBottom: fraction(state.smoothBottom),
    sharpCorners: fields.sharpCorners && state.sharpCorners,
    blendInner: fields.blendInner && state.blendInner,
    scale: fields.scale && state.scaleOn,
    scaleHeight: lengthValue(state.scaleHeight, units) ?? 5,
  };
}

/** The vectors a shape is built from: the selection, or when nothing is
 * selected the ones the edited shape was made from. */
export function shapeVectors(
  selection: readonly number[],
  editing: ComponentSummary | null,
): number[] {
  const source = editing?.shape ?? null;
  if (selection.length > 0 || source === null) {
    return [...selection];
  }
  const { vectors, profileVector } = source;
  return profileVector === undefined ? [...vectors] : [...vectors, profileVector];
}

/** The `model.createShape` arguments, or null when a field is wrong. A
 * fade or tilt still waiting for its anchors is left out. */
export function shapeArgs(
  state: ShapeFormState,
  tool: ShapeTool,
  units: Units,
  ids: readonly number[],
): ShapeArgs | null {
  const params = shapeParams(state, tool, units);
  if (params === null) {
    return null;
  }
  const fade = fadeProp(state.dynamic);
  const tilt = tiltProp(state.dynamic);
  const name = state.name.trim();
  return {
    ids: [...ids],
    params,
    combine: state.combine,
    baseHeight: lengthValue(state.baseHeight, units) ?? 0,
    ...(name === '' ? {} : { name }),
    ...(fade === null || fade === undefined ? {} : { fade }),
    ...(tilt === null || tilt === undefined ? {} : { tilt }),
  };
}

/** Whether Apply can run: vectors to build from (a flat plane covers the
 * material without any). */
/** What the form knows of a selected entity: whether it is an open or
 * closed vector (text, groups and the like count as closed shapes). */
export interface ShapeEntity {
  id: number;
  kind: string;
  closed: boolean;
}

/** Whether the tool has what it needs: closed vectors (a plane also takes
 * none, for the whole material), and for a custom shape one open profile
 * as well. Entities `entities` does not list count as closed. */
export function canApply(
  tool: ShapeTool,
  ids: readonly number[],
  entities: readonly ShapeEntity[] = [],
): boolean {
  const open = ids.filter((id) => {
    const entity = entities.find((e) => e.id === id);
    return entity !== undefined && entity.kind === 'path' && !entity.closed;
  });
  const closed = ids.length - open.length;
  if (tool === 'flat') {
    return ids.length === 0 || closed > 0;
  }
  if (tool === 'custom') {
    return closed > 0 && open.length === 1;
  }
  return closed > 0;
}

/** A typed Total Height: the scale and base height grow or shrink
 * together so the shape reaches it. `shapeHeight` is the rise the last
 * Apply gave (mm), null before any. */
export function withTotalHeight(
  state: ShapeFormState,
  totalText: string,
  shapeHeight: number | null,
  units: Units,
): ShapeFormState {
  const total = lengthValue(totalText, units);
  const base = lengthValue(state.baseHeight, units) ?? 0;
  if (total === null || total <= 0) {
    return state;
  }
  const shape = shapeHeight ?? (state.scaleOn ? lengthValue(state.scaleHeight, units) : null);
  if (shape === null || shape + base <= 0) {
    return {
      ...state,
      scaleOn: true,
      scaleHeight: lengthText(Math.max(total - base, 0.001), units),
    };
  }
  const factor = total / (shape + base);
  return {
    ...state,
    scaleOn: true,
    scaleHeight: lengthText(shape * factor, units),
    baseHeight: lengthText(base * factor, units),
  };
}

/** Form titles and hints per tool. */
export function toolText(tool: ShapeTool): { title: string; hint: string } {
  const t = strings.model.createShape.tools[tool];
  return { title: t.title, hint: t.hint };
}
