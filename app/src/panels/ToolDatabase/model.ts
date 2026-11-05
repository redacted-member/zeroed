// Pure state for the tool database editor: which geometry each tool type
// has, the fields as typed in the tool's own unit, the kernel tool and
// cutting data they make, and where a drag in the tree drops an item.
// Stored lengths are millimetres; feed rates are in the tool's feed unit,
// as the database keeps them.

import type { CuttingData, Tool, ToolDatabaseList, ToolKind } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { fieldError, lengthText, lengthValue, numberValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';

/** Tool types the editor offers, in order. */
export const EDITABLE_KINDS: readonly ToolKind[] = [
  'vBit',
  'engravingTool',
  'taperedBallNose',
  'ballNose',
  'endMill',
  'radiusedEndMill',
  'formTool',
  'diamondDrag',
  'drill',
  'threadMill',
  'laserTool',
];

export type GeometryField =
  | 'diameter'
  | 'includedAngle'
  | 'flatDiameter'
  | 'tipRadius'
  | 'taperAngle'
  | 'cornerRadius'
  | 'angle'
  | 'kerf'
  | 'power'
  | 'speed'
  | 'maxBurnRate'
  | 'pitch'
  | 'toothOffset'
  | 'teeth';

/** Geometry fields per type; angles are degrees, power a percentage, speeds (a laser's
 * speed and Maximum Burn Rate) mm per minute, the rest lengths. */
export const GEOMETRY: Record<string, readonly GeometryField[]> = {
  endMill: ['diameter'],
  ballNose: ['diameter'],
  radiusedEndMill: ['diameter', 'cornerRadius'],
  vBit: ['diameter', 'includedAngle'],
  engravingTool: ['diameter', 'includedAngle', 'flatDiameter'],
  taperedBallNose: ['diameter', 'tipRadius', 'taperAngle'],
  drill: ['diameter'],
  diamondDrag: ['diameter', 'angle'],
  formTool: [],
  laserTool: ['kerf', 'power', 'speed', 'maxBurnRate'],
  threadMill: ['diameter', 'pitch', 'includedAngle', 'toothOffset', 'teeth'],
};

const ANGLES: ReadonlySet<GeometryField> = new Set(['includedAngle', 'taperAngle', 'angle']);
/** Fields typed as plain numbers, not converted with the unit. */
const PLAIN: ReadonlySet<GeometryField> = new Set([
  ...ANGLES,
  'power',
  'speed',
  'maxBurnRate',
  'teeth',
]);

/** Whether a geometry field is an angle in degrees. */
export function isAngle(field: GeometryField): boolean {
  return ANGLES.has(field);
}

export const FEED_UNITS = [
  'mmPerSec',
  'mmPerMin',
  'metrePerMin',
  'inchPerSec',
  'inchPerMin',
] as const;
export type FeedUnit = (typeof FEED_UNITS)[number];

const MM_PER_MIN: Record<FeedUnit, number> = {
  mmPerSec: 60,
  mmPerMin: 1,
  metrePerMin: 1000,
  inchPerSec: 25.4 * 60,
  inchPerMin: 25.4,
};

export interface ToolFormState {
  name: string;
  number: string;
  kind: ToolKind;
  units: Units;
  geometry: Partial<Record<GeometryField, string>>;
  /** Half section of a form cutter, as the kernel stores it; null until one is set. */
  profile: unknown;
  flutes: string;
  passDepth: string;
  stepoverPercent: string;
  /** Stepover as a distance, kept in step with the percentage. */
  stepover: string;
  spindleSpeed: string;
  feedUnits: FeedUnit;
  feedRate: string;
  plungeRate: string;
  notes: string;
  /** The shank and holder the preview checks, as typed (zero leaves a part unchecked). */
  holder: Record<HolderField, string>;
  /** The tool cuts at its center, so it may plunge. */
  centerCutting: boolean;
}

/** The shank and holder lengths of the tool form, all in the tool's unit. */
export const HOLDER_FIELDS = [
  'fluteLength',
  'shankDiameter',
  'lengthBelowHolder',
  'holderDiameter',
  'holderLength',
] as const;
export type HolderField = (typeof HOLDER_FIELDS)[number];

export type ToolField =
  | 'name'
  | 'number'
  | 'flutes'
  | 'passDepth'
  | 'stepoverPercent'
  | 'stepover'
  | 'spindleSpeed'
  | 'feedRate'
  | 'plungeRate'
  | 'profile'
  | HolderField;
export type ToolErrors = Partial<Record<ToolField | GeometryField, string>>;

/** The machine and material the editor shows cutting data for. */
export interface CuttingContext {
  machine: string;
  material: string;
}

function isFeedUnit(value: string): value is FeedUnit {
  return (FEED_UNITS as readonly string[]).includes(value);
}

function round(value: number, places: number): number {
  const scale = 10 ** places;
  return Math.round(value * scale) / scale;
}

function geometryText(field: GeometryField, value: number, units: Units): string {
  return PLAIN.has(field) ? String(value) : lengthText(value, units);
}

/**
 * Form state for a tool, in the tool's own unit. Geometry and name come from
 * the stored tool; the cutting fields from `cutting`, the tool resolved for
 * the context, when given.
 */
export function toolFormState(tool: Tool, cutting: Tool = tool): ToolFormState {
  const units: Units = tool.units === 'inch' ? 'inch' : 'mm';
  const geometry: Partial<Record<GeometryField, string>> = {};
  for (const field of GEOMETRY[tool.toolType.kind] ?? ['diameter']) {
    const value = tool.toolType[field];
    // A laser tool saved before it had a Maximum Burn Rate reads the default.
    geometry[field] =
      typeof value === 'number'
        ? geometryText(field, value, units)
        : field === 'maxBurnRate'
          ? defaultGeometry(field, units)
          : '';
  }
  const profile = tool.toolType.kind === 'formTool' ? (tool.toolType['profile'] ?? null) : null;
  return {
    name: tool.name,
    number: String(cutting.number),
    kind: tool.toolType.kind,
    units,
    geometry,
    profile,
    flutes: String(tool.flutes ?? 0),
    passDepth: lengthText(cutting.passDepth, units),
    stepoverPercent: String(round(cutting.stepoverPercent, 2)),
    stepover: lengthText(cutting.stepover, units),
    spindleSpeed: String(cutting.spindleSpeed),
    feedUnits: isFeedUnit(cutting.feedUnits) ? cutting.feedUnits : 'mmPerMin',
    feedRate: String(cutting.feedRate),
    plungeRate: String(cutting.plungeRate),
    notes: tool.notes,
    holder: Object.fromEntries(
      HOLDER_FIELDS.map((field) => [field, lengthText(tool[field] ?? 0, units)]),
    ) as Record<HolderField, string>,
    centerCutting: tool.centerCutting ?? true,
  };
}

/** Sets one shank or holder field. */
export function setHolder(state: ToolFormState, field: HolderField, text: string): ToolFormState {
  return { ...state, holder: { ...state.holder, [field]: text } };
}

/** Cutting diameter in millimetres from the form, or null when not typed yet. */
export function formDiameter(state: ToolFormState): number | null {
  if (state.kind === 'laserTool') {
    return lengthValue(state.geometry.kerf ?? '', state.units);
  }
  return lengthValue(state.geometry.diameter ?? '', state.units);
}

/** Sets the stepover percentage and recomputes the distance. */
export function setStepoverPercent(state: ToolFormState, text: string): ToolFormState {
  const percent = numberValue(text);
  const diameter = formDiameter(state);
  if (percent === null || diameter === null) {
    return { ...state, stepoverPercent: text };
  }
  return {
    ...state,
    stepoverPercent: text,
    stepover: lengthText((diameter * percent) / 100, state.units),
  };
}

/** Sets the stepover distance and recomputes the percentage. */
export function setStepoverDistance(state: ToolFormState, text: string): ToolFormState {
  const distance = lengthValue(text, state.units);
  const diameter = formDiameter(state);
  if (distance === null || diameter === null || diameter <= 0) {
    return { ...state, stepover: text };
  }
  return {
    ...state,
    stepover: text,
    stepoverPercent: String(round((distance / diameter) * 100, 2)),
  };
}

/** Sets one geometry field, keeping the stepover percentage and moving the distance. */
export function setGeometry(
  state: ToolFormState,
  field: GeometryField,
  text: string,
): ToolFormState {
  const next = { ...state, geometry: { ...state.geometry, [field]: text } };
  return field === 'diameter' || field === 'kerf'
    ? setStepoverPercent(next, next.stepoverPercent)
    : next;
}

/**
 * Switches the tool type, keeping the fields both types share. A name still
 * at the old type's format takes the new type's.
 */
export function switchKind(
  state: ToolFormState,
  kind: ToolKind,
  formats: Partial<Record<ToolKind, string>> = {},
): ToolFormState {
  const geometry: Partial<Record<GeometryField, string>> = {};
  for (const field of GEOMETRY[kind] ?? ['diameter']) {
    geometry[field] = state.geometry[field] ?? defaultGeometry(field, state.units);
  }
  const oldFormat = formats[state.kind];
  const newFormat = formats[kind];
  const name =
    oldFormat !== undefined && newFormat !== undefined && state.name === oldFormat
      ? newFormat
      : state.name;
  return { ...state, kind, geometry, name, profile: kind === 'formTool' ? state.profile : null };
}

function defaultGeometry(field: GeometryField, units: Units): string {
  switch (field) {
    case 'includedAngle':
      return '90';
    case 'taperAngle':
      return '5';
    case 'angle':
      return '90';
    case 'flatDiameter':
      return lengthText(0.2, units);
    case 'tipRadius':
    case 'cornerRadius':
      return lengthText(1, units);
    case 'diameter':
      return lengthText(6, units);
    case 'kerf':
      return lengthText(0.2, units);
    case 'power':
      return '80';
    case 'speed':
      return '1200';
    case 'maxBurnRate':
      return '3000';
    case 'pitch':
      return lengthText(1.5, units);
    case 'toothOffset':
      return lengthText(0, units);
    case 'teeth':
      return '1';
  }
}

/** Switches the unit geometry and depths are typed in, converting them. */
export function switchUnits(state: ToolFormState, units: Units): ToolFormState {
  if (units === state.units) {
    return state;
  }
  const convert = (text: string) => {
    const mm = lengthValue(text, state.units);
    return mm === null ? text : lengthText(mm, units);
  };
  const geometry: Partial<Record<GeometryField, string>> = {};
  for (const [field, text] of Object.entries(state.geometry) as Array<[GeometryField, string]>) {
    geometry[field] = PLAIN.has(field) ? text : convert(text);
  }
  return {
    ...state,
    units,
    geometry,
    passDepth: convert(state.passDepth),
    stepover: convert(state.stepover),
    holder: Object.fromEntries(
      HOLDER_FIELDS.map((field) => [field, convert(state.holder[field])]),
    ) as Record<HolderField, string>,
  };
}

export function validateTool(state: ToolFormState): ToolErrors {
  const errors: ToolErrors = {};
  if (state.name.trim() === '') {
    errors.name = strings.toolDatabase.errorName;
  }
  const checks: Array<[ToolField | GeometryField, string | undefined]> = [
    ['number', fieldError(numberValue(state.number), { min: 1, max: 999, integer: true })],
    ['flutes', fieldError(numberValue(state.flutes), { min: 0, max: 99, integer: true })],
    ['passDepth', fieldError(lengthValue(state.passDepth, state.units), { positive: true })],
    [
      'stepoverPercent',
      fieldError(numberValue(state.stepoverPercent), { positive: true, max: 100 }),
    ],
    ['spindleSpeed', fieldError(numberValue(state.spindleSpeed), { min: 0 })],
    ['feedRate', fieldError(numberValue(state.feedRate), { positive: true })],
    ['plungeRate', fieldError(numberValue(state.plungeRate), { positive: true })],
  ];
  for (const field of GEOMETRY[state.kind] ?? []) {
    const text = state.geometry[field] ?? '';
    const value = PLAIN.has(field) ? numberValue(text) : lengthValue(text, state.units);
    const rule =
      field === 'flatDiameter' || field === 'toothOffset'
        ? { min: 0 }
        : field === 'teeth'
          ? { positive: true, integer: true, max: 99 }
          : ANGLES.has(field)
            ? { positive: true, max: 180 }
            : field === 'power'
              ? { positive: true, max: 100 }
              : { positive: true };
    checks.push([field, fieldError(value, rule)]);
  }
  for (const field of HOLDER_FIELDS) {
    checks.push([field, fieldError(lengthValue(state.holder[field], state.units), { min: 0 })]);
  }
  if (state.kind === 'formTool' && (state.profile === null || state.profile === undefined)) {
    checks.push(['profile', strings.toolDatabase.formProfileMissing]);
  }
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** The kernel tool the form describes, keeping what the form does not edit. */
export function toolFromState(state: ToolFormState, base: Tool): Tool | null {
  if (Object.keys(validateTool(state)).length > 0) {
    return null;
  }
  const toolType: Tool['toolType'] = { kind: state.kind };
  for (const field of GEOMETRY[state.kind] ?? []) {
    const text = state.geometry[field] ?? '';
    toolType[field] = (PLAIN.has(field) ? numberValue(text) : lengthValue(text, state.units)) ?? 0;
  }
  if (state.kind === 'formTool') {
    toolType['profile'] = state.profile;
  }
  const diameter = formDiameter(state) ?? 0;
  const percent = numberValue(state.stepoverPercent) ?? 40;
  return {
    ...base,
    name: state.name.trim(),
    number: numberValue(state.number) ?? 1,
    toolType,
    units: state.units,
    flutes: numberValue(state.flutes) ?? 0,
    passDepth: lengthValue(state.passDepth, state.units) ?? 0,
    stepoverPercent: percent,
    stepover: (diameter * percent) / 100,
    spindleSpeed: numberValue(state.spindleSpeed) ?? 0,
    feedUnits: state.feedUnits,
    feedRate: numberValue(state.feedRate) ?? 0,
    plungeRate: numberValue(state.plungeRate) ?? 0,
    notes: state.notes,
    ...holderOf(state, base),
  };
}

/** The shank and holder fields the form sets on `base`: a field the tool
 * did not have stays out while it keeps its default (zero, center cutting). */
function holderOf(state: ToolFormState, base: Tool): Partial<Tool> {
  const out: Partial<Tool> = {};
  for (const field of HOLDER_FIELDS) {
    const value = lengthValue(state.holder[field], state.units) ?? 0;
    if (value !== 0 || base[field] !== undefined) {
      out[field] = value;
    }
  }
  if (!state.centerCutting || base.centerCutting !== undefined) {
    out.centerCutting = state.centerCutting;
  }
  return out;
}

/** What Apply writes: the tool, and cutting data for the context when a material is active. */
export interface SavePlan {
  tool: Tool;
  cuttingData: CuttingData | null;
}

/**
 * Splits the form into what is stored on the tool and what belongs to the
 * machine and material pair. With no material active the cutting fields are
 * the tool's own, used for any material.
 */
export function savePlan(
  state: ToolFormState,
  stored: Tool,
  context: CuttingContext,
): SavePlan | null {
  const built = toolFromState(state, stored);
  if (built === null) {
    return null;
  }
  if (context.material === '') {
    return { tool: built, cuttingData: null };
  }
  const tool: Tool = {
    ...built,
    number: stored.number,
    passDepth: stored.passDepth,
    stepover: stored.stepover,
    stepoverPercent: stored.stepoverPercent,
    spindleSpeed: stored.spindleSpeed,
    feedUnits: stored.feedUnits,
    feedRate: stored.feedRate,
    plungeRate: stored.plungeRate,
  };
  const number = built.number === stored.number ? null : built.number;
  return {
    tool,
    cuttingData: {
      machine: context.machine,
      material: context.material,
      feedUnits: built.feedUnits,
      feedRate: built.feedRate,
      plungeRate: built.plungeRate,
      spindleSpeed: built.spindleSpeed,
      passDepth: built.passDepth,
      stepover: built.stepover,
      number,
    },
  };
}

/** Chip load in the tool's unit per tooth, or null without flutes, speed or feed. */
export function chipLoad(state: ToolFormState): number | null {
  const flutes = numberValue(state.flutes);
  const rpm = numberValue(state.spindleSpeed);
  const feed = numberValue(state.feedRate);
  if (flutes === null || rpm === null || feed === null || flutes <= 0 || rpm <= 0 || feed <= 0) {
    return null;
  }
  const mmPerTooth = (feed * MM_PER_MIN[state.feedUnits]) / (rpm * flutes);
  return state.units === 'inch' ? mmPerTooth / 25.4 : mmPerTooth;
}

/** The web links written in a tool's notes, in order, without repeats. */
export function notesLinks(notes: string): string[] {
  const found = notes.match(/https?:\/\/[^\s<>"')]+/g) ?? [];
  return [...new Set(found.map((link) => link.replace(/[.,;:]+$/, '')))];
}

/** A name with `{variable}` added at the end, spaced from what is there. */
export function insertVariable(name: string, variable: string): string {
  const token = `{${variable}}`;
  if (name === '' || name.endsWith(' ')) {
    return `${name}${token}`;
  }
  return `${name} ${token}`;
}

/** Whether a tool name uses the variable `name`, with or without a modifier. */
export function usesVariable(toolName: string, name: string): boolean {
  return toolName.includes(`{${name}}`) || toolName.includes(`{${name}|`);
}

/** Whether a custom variable name is usable inside braces. */
export function validVariableName(name: string): boolean {
  const trimmed = name.trim();
  return trimmed !== '' && !/[{}|]/.test(trimmed);
}

/** A new tool of a type, named with that type's format. */
export function blankTool(group: number, kind: ToolKind = 'endMill', format?: string): Tool {
  const fields = GEOMETRY[kind] ?? ['diameter'];
  const toolType: Tool['toolType'] = { kind };
  for (const field of fields) {
    const text = defaultGeometry(field, 'mm');
    toolType[field] = Number(text);
  }
  if (kind === 'formTool') {
    toolType['profile'] = null;
  }
  const diameter =
    typeof toolType['diameter'] === 'number'
      ? toolType['diameter']
      : typeof toolType['kerf'] === 'number'
        ? toolType['kerf']
        : 6;
  return {
    id: 0,
    name: format ?? strings.toolDatabase.newToolName,
    number: 1,
    group,
    toolType,
    passDepth: 3,
    stepover: (diameter * 40) / 100,
    stepoverPercent: 40,
    feedRate: 2400,
    plungeRate: 600,
    feedUnits: 'mmPerMin',
    spindleSpeed: 18000,
    notes: '',
    units: 'mm',
    clearance: 0,
    cuttingData: [],
    flutes: 2,
    variables: [],
  };
}

/** An item of the tool tree. */
export type TreeItem = { kind: 'tool'; id: number } | { kind: 'group'; id: number };

/** Where a drop lands: into a group, before a sibling or last. */
export interface Placement {
  item: TreeItem;
  /** Group the item ends up in; 0 for the top level. */
  group: number;
  /** Sibling of the same kind it goes before, null for last. */
  before: number | null;
}

function groupOf(database: ToolDatabaseList, item: TreeItem): number {
  if (item.kind === 'tool') {
    return database.tools.find((t) => t.id === item.id)?.group ?? 0;
  }
  return database.groups.find((g) => g.id === item.id)?.parent ?? 0;
}

/** Whether `group` is `ancestor` or sits somewhere inside it. */
function within(database: ToolDatabaseList, group: number, ancestor: number): boolean {
  let current: number | null = group;
  for (let guard = 0; current !== null && current !== 0 && guard < 64; guard += 1) {
    if (current === ancestor) {
      return true;
    }
    const at: number = current;
    current = database.groups.find((g) => g.id === at)?.parent ?? null;
  }
  return false;
}

/**
 * Where dropping `dragged` on `target` puts it. A drop on a group moves the
 * item into it, at the end; a drop on an item of the same kind puts it just
 * before that item, in its group; a tool dropped on a tool of another group
 * joins that group. Null when the drop would do nothing or is not allowed
 * (a group into itself).
 */
export function placement(
  database: ToolDatabaseList,
  dragged: TreeItem,
  target: TreeItem | 'top',
): Placement | null {
  if (target !== 'top' && target.kind === dragged.kind && target.id === dragged.id) {
    return null;
  }
  let result: Placement;
  if (target === 'top') {
    result = { item: dragged, group: 0, before: null };
  } else if (target.kind === 'group' && dragged.kind === 'tool') {
    result = { item: dragged, group: target.id, before: null };
  } else if (target.kind === 'group') {
    // A group dropped on a group goes before it, beside it.
    result = { item: dragged, group: groupOf(database, target), before: target.id };
  } else if (dragged.kind === 'tool') {
    result = { item: dragged, group: groupOf(database, target), before: target.id };
  } else {
    // A group dropped on a tool joins that tool's group, last.
    result = { item: dragged, group: groupOf(database, target), before: null };
  }
  if (dragged.kind === 'group' && within(database, result.group, dragged.id)) {
    return null;
  }
  return result;
}

/** Siblings of an item in list order. */
function siblings(database: ToolDatabaseList, item: TreeItem): number[] {
  const group = groupOf(database, item);
  return item.kind === 'tool'
    ? database.tools.filter((t) => (t.group ?? 0) === group).map((t) => t.id)
    : database.groups.filter((g) => (g.parent ?? 0) === group).map((g) => g.id);
}

/** Where Alt with an arrow key moves an item: one place up or down among its siblings. */
export function nudge(
  database: ToolDatabaseList,
  item: TreeItem,
  direction: 'up' | 'down',
): Placement | null {
  const list = siblings(database, item);
  const index = list.indexOf(item.id);
  if (index < 0) {
    return null;
  }
  const group = groupOf(database, item);
  if (direction === 'up') {
    const before = list[index - 1];
    return before === undefined ? null : { item, group, before };
  }
  if (index >= list.length - 1) {
    return null;
  }
  return { item, group, before: list[index + 2] ?? null };
}

/** The saved name of a group, and the key its editor is made with: a new
 * key whenever the name the database holds changes, so a group picked
 * before the database listed it shows its name once it does. */
export function groupEditorName(
  database: ToolDatabaseList,
  id: number,
): { name: string; key: string } {
  const name = database.groups.find((g) => g.id === id)?.name ?? '';
  return { name, key: `${id}:${name}` };
}

/** Group names from the top level down to a tool's group, for its breadcrumb. */
export function breadcrumb(database: ToolDatabaseList, item: TreeItem): string[] {
  const group = groupOf(database, item);
  return database.groups.find((g) => g.id === group)?.path ?? [];
}

/** The database as a picking form lists it: only the tools of `kinds` when
 * the form takes only those, and every group. */
export function pickableDatabase(
  database: ToolDatabaseList,
  kinds: readonly ToolKind[] | null,
): ToolDatabaseList {
  if (kinds === null) {
    return database;
  }
  return { ...database, tools: database.tools.filter((t) => kinds.includes(t.toolType.kind)) };
}
