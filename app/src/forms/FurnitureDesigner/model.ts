// The Furniture Designer form's pure state: the parameter fields (text in
// the job unit, calculations allowed), their groups and sliders, the joint
// choices (style, per class, per connection) and the lists as CSV.

import type {
  AttachmentRow,
  ConnectionRow,
  CutListRow,
  FurnitureEstimate,
  FurnitureJoint,
  FurnitureJoints,
  FurnitureParam,
  FurnitureValues,
  HardwareRow,
  JoineryStyle,
  JointChoice,
  JointClass,
  JointSource,
  StockThickness,
} from '../../bridge/furniture';
import type { JointParams } from '../../bridge/joints';
import type { Units } from '../../bridge/types';
import { furnitureStrings } from '../../lib/strings/furniture';
import { formatLength, parseLength, toDisplay, toMillimetres, unitSuffix } from '../../lib/units';
import { JOINT_KINDS, type NumberSpec } from '../JointCreatorForm/model';
import { csvField } from '../AssemblyViewForm/model';

const text = furnitureStrings.furniture.designer;

/** Kinds whose values are millimetres, shown in the job unit. */
function isLength(param: FurnitureParam): boolean {
  return param.kind === 'length' || param.kind === 'thickness';
}

/** A value as its field shows it: lengths in the job unit. */
export function fieldText(param: FurnitureParam, value: number | string, units: Units): string {
  if (typeof value === 'string') {
    return value;
  }
  return isLength(param) ? formatLength(value, units) : String(value);
}

/** Every parameter's field text for these values (defaults where a value is missing). */
export function fieldsOf(
  params: readonly FurnitureParam[],
  values: FurnitureValues,
  units: Units,
): Record<string, string> {
  const fields: Record<string, string> = {};
  for (const p of params) {
    fields[p.key] = fieldText(p, values[p.key] ?? p.default, units);
  }
  return fields;
}

export type Parsed = { ok: true; value: number | string } | { ok: false; error: string };

/** A bound as the error line shows it. */
function boundText(param: FurnitureParam, bound: number, units: Units): string {
  return isLength(param) ? formatLength(bound, units) : String(bound);
}

/**
 * The value a field's text gives: lengths typed in the job unit (with
 * calculations) become millimetres, counts must be whole, toggles 0 or 1,
 * a choice one of its options; min and max are inclusive.
 */
export function parseParam(param: FurnitureParam, typed: string, units: Units): Parsed {
  if (param.kind === 'choice') {
    return (param.options ?? []).includes(typed)
      ? { ok: true, value: typed }
      : { ok: false, error: text.badValue };
  }
  if (param.kind === 'toggle') {
    return typed === '0' || typed === '1'
      ? { ok: true, value: Number(typed) }
      : { ok: false, error: text.badValue };
  }
  const number = parseLength(typed);
  if (number === null || !Number.isFinite(number)) {
    return { ok: false, error: text.badValue };
  }
  const value = isLength(param) ? toMillimetres(number, units) : number;
  if (param.kind === 'count' && !Number.isInteger(value)) {
    return { ok: false, error: text.wholeNumber };
  }
  // A small allowance for the inch round trip of a bound typed back.
  const slack = isLength(param) && units === 'inch' ? 1e-6 : 0;
  const low = param.min !== undefined && value < param.min - slack;
  const high = param.max !== undefined && value > param.max + slack;
  if (low || high) {
    const min = param.min === undefined ? '' : boundText(param, param.min, units);
    const max = param.max === undefined ? '' : boundText(param, param.max, units);
    return { ok: false, error: text.between(min, max) };
  }
  return { ok: true, value };
}

/**
 * The fields after the values went from `before` to `values` (a size
 * arrow dragged in the view): only fields whose value changed follow, and
 * one whose text already gives the new value keeps its text (a typed
 * calculation stays); a field still being typed with an error is left.
 */
export function followValues(
  params: readonly FurnitureParam[],
  fields: Readonly<Record<string, string>>,
  before: FurnitureValues,
  values: FurnitureValues,
  units: Units,
): Record<string, string> {
  const next: Record<string, string> = { ...fields };
  for (const p of params) {
    const value = values[p.key];
    if (value === undefined || value === before[p.key]) {
      continue;
    }
    const parsed = parseParam(p, fields[p.key] ?? '', units);
    const same =
      parsed.ok &&
      (typeof value === 'number' && typeof parsed.value === 'number'
        ? Math.abs(parsed.value - value) < 1e-9
        : parsed.value === value);
    if (!same) {
      next[p.key] = fieldText(p, value, units);
    }
  }
  return next;
}

export interface ValuesResult {
  values: FurnitureValues;
  /** The error of each field that does not read, by key. */
  errors: Record<string, string>;
}

/** The values the fields give; a bad field keeps its last good value and reports its error. */
export function valuesOf(
  params: readonly FurnitureParam[],
  fields: Readonly<Record<string, string>>,
  last: FurnitureValues,
  units: Units,
): ValuesResult {
  const values: FurnitureValues = {};
  const errors: Record<string, string> = {};
  for (const p of params) {
    const parsed = parseParam(p, fields[p.key] ?? '', units);
    if (parsed.ok) {
      values[p.key] = parsed.value;
    } else {
      errors[p.key] = parsed.error;
      values[p.key] = last[p.key] ?? p.default;
    }
  }
  return { values, errors };
}

export interface ParamGroup {
  /** The group's heading; empty for parameters with none. */
  group: string;
  params: FurnitureParam[];
}

/** The parameters by group, groups in the order they first appear. */
export function paramGroups(params: readonly FurnitureParam[]): ParamGroup[] {
  const groups: ParamGroup[] = [];
  for (const p of params) {
    const name = p.group ?? '';
    let group = groups.find((g) => g.group === name);
    if (group === undefined) {
      group = { group: name, params: [] };
      groups.push(group);
    }
    group.params.push(p);
  }
  return groups;
}

export interface SliderSpec {
  /** In the job unit for lengths. */
  min: number;
  max: number;
  step: number;
}

/** A stock thickness offered for a thickness parameter: its label and its text in the job unit. */
export interface StockChoice {
  id: string;
  label: string;
  text: string;
}

/**
 * The stock thicknesses a thickness parameter can take (what each usually
 * measures, within the parameter's bounds), the job unit's system first and
 * thinnest first within each.
 */
export function stockChoices(
  param: FurnitureParam,
  stocks: readonly StockThickness[],
  units: Units,
): StockChoice[] {
  if (param.kind !== 'thickness') {
    return [];
  }
  const own = units === 'inch' ? 'inch' : 'metric';
  return stocks
    .filter(
      (s) =>
        (param.min === undefined || s.actual >= param.min) &&
        (param.max === undefined || s.actual <= param.max),
    )
    .sort((a, b) => Number(b.system === own) - Number(a.system === own) || a.actual - b.actual)
    .map((s) => {
      const size = `${formatLength(s.actual, units)} ${unitSuffix(units)}`;
      return {
        id: s.id,
        label: s.label.includes(size) ? s.label : `${s.label}, ${size}`,
        text: formatLength(s.actual, units),
      };
    });
}

/** The slider under a length or count with both bounds; null for the rest. */
export function sliderSpec(param: FurnitureParam, units: Units): SliderSpec | null {
  if ((param.kind !== 'length' && param.kind !== 'count') || param.min === undefined) {
    return null;
  }
  if (param.max === undefined || param.max <= param.min) {
    return null;
  }
  if (param.kind === 'count') {
    return { min: param.min, max: param.max, step: 1 };
  }
  return {
    min: toDisplay(param.min, units),
    max: toDisplay(param.max, units),
    step: units === 'inch' ? 0.0625 : 1,
  };
}

/** Where the slider stands for a field's text, or its default when the text does not read. */
export function sliderValue(param: FurnitureParam, typed: string, units: Units): number {
  const parsed = parseParam(param, typed, units);
  const mm = parsed.ok && typeof parsed.value === 'number' ? parsed.value : Number(param.default);
  return isLength(param) ? toDisplay(mm, units) : mm;
}

/** The field text for a slider position (job unit). */
export function sliderText(param: FurnitureParam, position: number, units: Units): string {
  return isLength(param) ? formatLength(toMillimetres(position, units), units) : String(position);
}

/** Joints with nothing chosen yet: the style only. */
export function initialJoints(style: JoineryStyle = 'template'): FurnitureJoints {
  return { style, classes: {}, connections: {} };
}

/** The joints with another style; the overrides stay, so they still win. */
export function withStyle(joints: FurnitureJoints, style: JoineryStyle): FurnitureJoints {
  return { ...joints, style };
}

/**
 * The joints with a class's joint set (null drops the class override).
 * `clearing` are connections whose own overrides go, so every one of the
 * class ends up with the joint ("Use for all like this").
 */
export function withClassJoint(
  joints: FurnitureJoints,
  cls: string,
  joint: FurnitureJoint | null,
  clearing: readonly string[] = [],
): FurnitureJoints {
  const classes = { ...joints.classes };
  if (joint === null) {
    delete classes[cls];
  } else {
    // A new joint starts from its own sizes.
    classes[cls] = { joint };
  }
  const connections = { ...joints.connections };
  for (const key of clearing) {
    delete connections[key];
  }
  return { ...joints, classes, connections };
}

/** The joints with one connection's joint set (null drops its override). */
export function withConnectionJoint(
  joints: FurnitureJoints,
  key: string,
  joint: FurnitureJoint | null,
): FurnitureJoints {
  const connections = { ...joints.connections };
  if (joint === null) {
    delete connections[key];
  } else {
    connections[key] = { joint };
  }
  return { ...joints, connections };
}

/** The joints with a class's joint sizes changed (the joint becomes the class's own). */
export function withClassParams(
  joints: FurnitureJoints,
  cls: string,
  joint: FurnitureJoint,
  params: JointParams,
): FurnitureJoints {
  const kept: JointChoice = Object.keys(params).length === 0 ? { joint } : { joint, params };
  return { ...joints, classes: { ...joints.classes, [cls]: kept } };
}

/**
 * The overrides a made design keeps, read back from its connections: a
 * class joint from any connection whose joint comes from its class, a
 * connection joint from each one set by hand.
 */
export function jointsFromConnections(
  style: JoineryStyle,
  connections: readonly ConnectionRow[],
): FurnitureJoints {
  const classes: Record<string, JointChoice> = {};
  const own: Record<string, JointChoice> = {};
  const choice = (c: ConnectionRow): JointChoice =>
    Object.keys(c.params).length === 0 ? { joint: c.joint } : { joint: c.joint, params: c.params };
  for (const c of connections) {
    if (c.source === 'class' && classes[c.class] === undefined) {
      classes[c.class] = choice(c);
    } else if (c.source === 'connection') {
      own[c.key] = choice(c);
    }
  }
  return { style, classes, connections: own };
}

export interface ClassRow {
  cls: string;
  label: string;
  /** The joint the row shows. */
  joint: FurnitureJoint;
  /** The joints valid for every connection of the class. */
  choices: FurnitureJoint[];
  source: JointSource;
  /** The class's typed sizes. */
  params: JointParams;
  /** Connections of the class set one by one. */
  overridden: string[];
  /** Every connection of the class. */
  keys: string[];
}

/** The joint most connections show, the first one on a tie. */
function commonest(rows: readonly ConnectionRow[]): FurnitureJoint | null {
  const counts = new Map<FurnitureJoint, number>();
  let best: FurnitureJoint | null = null;
  let most = 0;
  for (const row of rows) {
    const n = (counts.get(row.joint) ?? 0) + 1;
    counts.set(row.joint, n);
    if (n > most) {
      most = n;
      best = row.joint;
    }
  }
  return best;
}

/**
 * One row per class that has a connection: the template's classes in
 * their order, then any other (such as "other"). The row's joint is the
 * class override, else the one the connections not set by hand show.
 */
export function classRows(
  classes: Readonly<Record<string, JointClass>> | undefined,
  connections: readonly ConnectionRow[],
  joints: FurnitureJoints,
  otherLabel: (cls: string) => string,
): ClassRow[] {
  const order = Object.keys(classes ?? {});
  for (const c of connections) {
    if (!order.includes(c.class)) {
      order.push(c.class);
    }
  }
  const rows: ClassRow[] = [];
  for (const cls of order) {
    const members = connections.filter((c) => c.class === cls);
    if (members.length === 0) {
      continue;
    }
    const first = members[0];
    const choices = first.choices.filter((j) => members.every((m) => m.choices.includes(j)));
    const own = joints.classes[cls];
    const free = members.filter((m) => joints.connections[m.key] === undefined);
    const joint = own?.joint ?? commonest(free.length > 0 ? free : members) ?? first.joint;
    rows.push({
      cls,
      label: classes?.[cls]?.label ?? otherLabel(cls),
      joint,
      choices: choices.includes(joint) ? choices : [joint, ...choices],
      source: own === undefined ? 'style' : 'class',
      params: own?.params ?? {},
      overridden: members.filter((m) => joints.connections[m.key] !== undefined).map((m) => m.key),
      keys: members.map((m) => m.key),
    });
  }
  return rows;
}

/** The size fields a joint offers under Edit joint sizes. */
export function jointSizeFields(joint: FurnitureJoint): readonly NumberSpec[] {
  switch (joint) {
    case 'none':
      return [{ key: 'count', unit: 'count', label: 'count' }];
    case 'camLock':
      return [
        { key: 'diameter', unit: 'length' },
        { key: 'depth', unit: 'length' },
        { key: 'inset', unit: 'length' },
      ];
    case 'stoppedDado':
      return [
        { key: 'depth', unit: 'length' },
        { key: 'stop', unit: 'length' },
      ];
    case 'groove':
      return [{ key: 'depth', unit: 'length' }];
    default:
      return JOINT_KINDS.find((k) => k.kind === joint)?.numbers ?? [];
  }
}

/** The sizes' text in the job unit (empty for a size left to the joint). */
export function sizeTexts(
  joint: FurnitureJoint,
  params: JointParams,
  units: Units,
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const spec of jointSizeFields(joint)) {
    const value = params[spec.key];
    out[spec.key] =
      typeof value !== 'number'
        ? ''
        : spec.unit === 'length'
          ? formatLength(value, units)
          : String(value);
  }
  return out;
}

/**
 * The sizes the texts give (empty ones left out), or the keys that do not
 * read: lengths in the job unit, counts whole and at least 1.
 */
export function sizesOf(
  joint: FurnitureJoint,
  texts: Readonly<Record<string, string>>,
  units: Units,
): { ok: true; params: JointParams } | { ok: false; bad: string[] } {
  const params: Record<string, number> = {};
  const bad: string[] = [];
  for (const spec of jointSizeFields(joint)) {
    const typed = (texts[spec.key] ?? '').trim();
    if (typed === '') {
      continue;
    }
    const value = parseLength(typed);
    if (value === null || value <= 0 || (spec.unit === 'count' && !Number.isInteger(value))) {
      bad.push(spec.key);
      continue;
    }
    params[spec.key] = spec.unit === 'length' ? toMillimetres(value, units) : value;
  }
  return bad.length > 0 ? { ok: false, bad } : { ok: true, params: params as JointParams };
}

/** A template id from its name: lower case words joined by dashes ("My Bookcase 2" is "my-bookcase-2"). */
export function templateId(name: string): string {
  const id = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return id === '' ? 'template' : id;
}

/** Whether two value sets are the same. */
export function sameValues(a: FurnitureValues, b: FurnitureValues): boolean {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
  for (const key of keys) {
    if (a[key] !== b[key]) {
      return false;
    }
  }
  return true;
}

/** The models (components) attached to the panels a cut list row counts. */
export function carvingCount(row: CutListRow, attachments: readonly AttachmentRow[]): number {
  const panels = new Set(row.panels);
  return attachments.filter((a) => a.kind === 'component' && panels.has(a.panel)).length;
}

/** A length for a CSV cell, in the job unit. */
function csvLength(mm: number, units: Units): string {
  return formatLength(mm, units);
}

/** The name of panel `key` from the cut list (its key when not listed). */
export function panelName(rows: readonly CutListRow[], key: string): string {
  for (const row of rows) {
    const i = row.panels.indexOf(key);
    if (i >= 0 && i < row.names.length) {
      return row.names[i];
    }
  }
  return key;
}

/** A cut list row's name: its panels' names. */
export function rowName(row: CutListRow): string {
  return row.names.join(', ');
}

/** The cut list as CSV in the job unit, a header line first. */
export function cutListCsv(rows: readonly CutListRow[], units: Units): string {
  const lines = [text.cutListHeader.map(csvField).join(',')];
  for (const r of rows) {
    lines.push(
      [
        rowName(r),
        r.materialLabel,
        String(r.quantity),
        csvLength(r.length, units),
        csvLength(r.width, units),
        csvLength(r.thickness, units),
        text.grain[r.grain],
        r.banding.length === 0 ? text.noBanding : r.banding.join(' '),
      ]
        .map(csvField)
        .join(','),
    );
  }
  return `${lines.join('\n')}\n`;
}

/** The hardware as CSV. */
export function hardwareCsv(rows: readonly HardwareRow[]): string {
  const lines = [text.hardwareHeader.map(csvField).join(',')];
  for (const r of rows) {
    lines.push([r.name, String(r.count), r.note].map(csvField).join(','));
  }
  return `${lines.join('\n')}\n`;
}

/** Area in the job unit squared: m² for mm jobs, ft² for inch jobs. */
export function areaText(mm2: number, units: Units): string {
  return units === 'inch'
    ? `${(mm2 / (304.8 * 304.8)).toFixed(2)} ft²`
    : `${(mm2 / 1e6).toFixed(2)} m²`;
}

/** The estimate as CSV. */
export function estimateCsv(estimate: FurnitureEstimate, units: Units): string {
  const lines = [text.estimateHeader.map(csvField).join(',')];
  for (const m of estimate.sheets) {
    lines.push(
      [
        m.label,
        csvLength(m.thickness, units),
        String(m.count),
        areaText(m.used, units),
        text.percent(m.waste),
      ]
        .map(csvField)
        .join(','),
    );
  }
  return `${lines.join('\n')}\n`;
}

/** A cutting time as h:mm:ss. */
export function timeText(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const rest = s % 60;
  return `${h}:${String(m).padStart(2, '0')}:${String(rest).padStart(2, '0')}`;
}
