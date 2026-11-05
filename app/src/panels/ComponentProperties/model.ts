// Pure state for the Component Properties form: one or more components, a
// group, or a level. Lengths are text in the display unit, the shape height
// a percentage; the patches carry only what changed.

import type { ComponentGroup, ComponentProps, GroupPatch } from '../../bridge/componentTree';
import type { Combine, ComponentSummary, Level, LevelPatch } from '../../bridge/model';
import type { Material3d } from '../../bridge/prefs';
import type { Units } from '../../bridge/types';
import { fieldError, lengthText, lengthValue, numberValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { materialLook } from '../../views/View3D/appearance';
import {
  dynamicState,
  fadeChanged,
  fadeProp,
  tiltChanged,
  tiltProp,
  validateDynamic,
  type DynamicState,
} from '../../forms/DynamicFields/model';

/** What the form edits. */
export type PropertiesTarget =
  | { kind: 'components'; ids: number[] }
  | { kind: 'group'; id: number }
  | { kind: 'level'; id: number };

/** How the item shows in the 3D view: like its parent (components; the
 * level, then the job's material), its own colour, a preview material, or
 * (groups and levels) the appearance of its members. */
export type Appearance = 'parent' | 'colour' | 'material' | 'children';

/** Preview materials an item can be given. */
export type PreviewMaterial = Exclude<Material3d, 'theme' | 'solid'>;

export const PREVIEW_MATERIALS: readonly PreviewMaterial[] = [
  'pine',
  'oak',
  'walnut',
  'maple',
  'mdf',
  'acrylic',
  'aluminium',
];

function previewMaterial(name: string | null | undefined): PreviewMaterial | null {
  return PREVIEW_MATERIALS.find((m) => m === name) ?? null;
}

export interface PropertiesState {
  name: string;
  combine: Combine;
  visible: boolean;
  /** Percent, components only. */
  shapeHeight: string;
  /** Display unit, components only. */
  baseHeight: string;
  appearance: Appearance;
  /** `#rrggbb`. */
  colour: string;
  material: PreviewMaterial;
  /** Components only: fade and tilt. */
  dynamic: DynamicState;
}

export type PropertiesErrors = Partial<
  Record<'name' | 'shapeHeight' | 'baseHeight' | 'fadePercent' | 'tiltAngle', string>
>;

/** Combine modes in the order and with the names the tree uses. */
export const TREE_COMBINES: ReadonlyArray<{ value: Combine; label: string }> = [
  { value: 'add', label: strings.model.combine.add },
  { value: 'subtract', label: strings.model.combine.subtract },
  { value: 'mergeHigh', label: strings.model.combine.mergeHigh },
  { value: 'mergeLow', label: strings.model.combine.mergeLow },
  { value: 'multiply', label: strings.model.combine.multiply },
];

/** Colour offered when switching away from the material. */
export const DEFAULT_COLOUR = '#c08040';

/** Material offered first. */
export const DEFAULT_MATERIAL: PreviewMaterial = 'oak';

/** The appearance, colour and material fields for a stored colour and
 * material; `none` is the appearance with neither. */
function appearanceOf(
  color: string | null | undefined,
  material: string | null | undefined,
  none: Appearance,
): Pick<PropertiesState, 'appearance' | 'colour' | 'material'> {
  const named = previewMaterial(material);
  if (color === null || color === undefined) {
    return { appearance: none, colour: DEFAULT_COLOUR, material: DEFAULT_MATERIAL };
  }
  return named === null
    ? { appearance: 'colour', colour: color, material: DEFAULT_MATERIAL }
    : { appearance: 'material', colour: color, material: named };
}

const COLOUR = /^#[0-9a-f]{6}$/;

function percentText(scale: number): string {
  return String(Math.round(scale * 1000) / 10);
}

/** The first component's values; the name only with one component. */
export function componentState(
  components: readonly ComponentSummary[],
  units: Units,
): PropertiesState {
  const first = components[0];
  return {
    name: components.length === 1 ? (first?.name ?? '') : '',
    combine: first?.combine ?? 'add',
    visible: first?.visible ?? true,
    shapeHeight: percentText(first?.heightScale ?? 1),
    baseHeight: lengthText(first?.baseHeight ?? 0, units),
    ...appearanceOf(first?.color, first?.material, 'parent'),
    dynamic: dynamicState(first?.fade, first?.tilt),
  };
}

export function groupState(group: ComponentGroup, units: Units): PropertiesState {
  return {
    name: group.name,
    combine: group.combine,
    visible: group.visible,
    shapeHeight: '100',
    baseHeight: lengthText(0, units),
    ...appearanceOf(group.color, group.material, 'children'),
    dynamic: dynamicState(),
  };
}

export function levelState(level: Level, units: Units): PropertiesState {
  return {
    name: level.name,
    combine: level.combine,
    visible: level.visible,
    shapeHeight: '100',
    baseHeight: lengthText(level.baseHeight ?? 0, units),
    ...appearanceOf(level.color, level.material, 'children'),
    dynamic: dynamicState(),
  };
}

/** Converts the base height when the display unit changes. */
export function convertPropertiesUnits(
  state: PropertiesState,
  from: Units,
  to: Units,
): PropertiesState {
  const mm = lengthValue(state.baseHeight, from);
  return mm === null ? state : { ...state, baseHeight: lengthText(mm, to) };
}

export function validateProperties(
  state: PropertiesState,
  kind: PropertiesTarget['kind'],
  count: number,
  units: Units,
): PropertiesErrors {
  const errors: PropertiesErrors = {};
  if ((kind !== 'components' || count === 1) && state.name.trim() === '') {
    errors.name = strings.model.properties.nameRequired;
  }
  if (kind === 'components' || kind === 'level') {
    const base = fieldError(lengthValue(state.baseHeight, units));
    if (base !== undefined) {
      errors.baseHeight = base;
    }
  }
  if (kind === 'components') {
    const shape = fieldError(numberValue(state.shapeHeight), { min: 0, max: 10000 });
    if (shape !== undefined) {
      errors.shapeHeight = shape;
    }
    Object.assign(errors, validateDynamic(state.dynamic));
  }
  return errors;
}

interface Look {
  color: string | null;
  material: string | null;
}

/** The colour and material name the state stands for. A material is sent
 * as its preview colour, with its name so the form can show it again. */
export function lookOf(state: PropertiesState): Look {
  if (state.appearance === 'material') {
    return { color: materialLook(state.material, '', '').colour, material: state.material };
  }
  if (state.appearance === 'colour' && COLOUR.test(state.colour.toLowerCase())) {
    return { color: state.colour.toLowerCase(), material: null };
  }
  return { color: null, material: null };
}

/** The colour and material fields that differ from `initial`. */
function lookPatch(
  state: PropertiesState,
  initial: PropertiesState,
): { color?: string | null; material?: string | null } {
  const now = lookOf(state);
  const was = lookOf(initial);
  const patch: { color?: string | null; material?: string | null } = {};
  if (now.color !== was.color) {
    patch.color = now.color;
  }
  if (now.material !== was.material) {
    patch.material = now.material;
  }
  return patch;
}

function close(a: number, b: number): boolean {
  return Math.abs(a - b) < 1e-9;
}

/** The `model.props` fields that differ from `initial`, or null when the
 * form has errors. With several components every changed field goes to
 * all of them, except the name. */
export function componentPatch(
  state: PropertiesState,
  initial: PropertiesState,
  count: number,
  units: Units,
): ComponentProps | null {
  if (Object.keys(validateProperties(state, 'components', count, units)).length > 0) {
    return null;
  }
  const patch: ComponentProps = {};
  if (count === 1 && state.name.trim() !== initial.name.trim()) {
    patch.name = state.name.trim();
  }
  if (state.combine !== initial.combine) {
    patch.combine = state.combine;
  }
  if (state.visible !== initial.visible) {
    patch.visible = state.visible;
  }
  const shape = numberValue(state.shapeHeight) ?? 100;
  if (!close(shape, numberValue(initial.shapeHeight) ?? 100)) {
    patch.heightScale = shape / 100;
  }
  const base = lengthValue(state.baseHeight, units) ?? 0;
  if (!close(base, lengthValue(initial.baseHeight, units) ?? 0)) {
    patch.baseHeight = base;
  }
  Object.assign(patch, lookPatch(state, initial));
  // A fade or tilt still waiting for its anchors changes nothing yet.
  const fade = fadeProp(state.dynamic);
  if (fade !== undefined && fadeChanged(fade, fadeProp(initial.dynamic) ?? null)) {
    patch.fade = fade;
  }
  const tilt = tiltProp(state.dynamic);
  if (tilt !== undefined && tiltChanged(tilt, tiltProp(initial.dynamic) ?? null)) {
    patch.tilt = tilt;
  }
  return patch;
}

/** The `model.groups.update` fields that differ, or null on errors. */
export function groupPatch(state: PropertiesState, initial: PropertiesState): GroupPatch | null {
  if (state.name.trim() === '') {
    return null;
  }
  const patch: GroupPatch = {};
  if (state.name.trim() !== initial.name.trim()) {
    patch.name = state.name.trim();
  }
  if (state.combine !== initial.combine) {
    patch.combine = state.combine;
  }
  if (state.visible !== initial.visible) {
    patch.visible = state.visible;
  }
  Object.assign(patch, lookPatch(state, initial));
  return patch;
}

/** The `model.levels.update` fields that differ, or null on errors. */
export function levelPatch(
  state: PropertiesState,
  initial: PropertiesState,
  units: Units,
): LevelPatch | null {
  const base = lengthValue(state.baseHeight, units);
  if (state.name.trim() === '' || base === null) {
    return null;
  }
  const patch: LevelPatch = {};
  if (state.baseHeight !== initial.baseHeight) {
    patch.baseHeight = base;
  }
  if (state.name.trim() !== initial.name.trim()) {
    patch.name = state.name.trim();
  }
  if (state.combine !== initial.combine) {
    patch.combine = state.combine;
  }
  if (state.visible !== initial.visible) {
    patch.visible = state.visible;
  }
  Object.assign(patch, lookPatch(state, initial));
  return patch;
}
