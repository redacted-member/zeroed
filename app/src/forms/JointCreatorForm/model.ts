// The Joint Creator panel's state, pure: the joint kind, the two picks, the
// joint's own sizes as text in the display unit (empty = worked out by the
// kernel) and what the hint line says. The kernel does the geometry.

import type {
  EdgeLine,
  Fit,
  HingePattern,
  JointKind,
  JointParams,
  JointRow,
  JointSpec,
  PickedEdge,
  Target,
} from '../../bridge/joints';
import type { Units } from '../../bridge/types';
import { lengthText, lengthValue, numberValue } from '../../lib/formFields';
import { jointsStrings } from '../../lib/strings/joints';
import { unitSuffix } from '../../lib/units';

/** Joint sizes typed as numbers. */
export type NumberKey =
  | 'count'
  | 'width'
  | 'depth'
  | 'stop'
  | 'proud'
  | 'detent'
  | 'angle'
  | 'radius'
  | 'diameter'
  | 'inset'
  | 'length'
  | 'nutWidth'
  | 'nutThickness'
  | 'atA'
  | 'atB'
  | 'depthB';

/** Joint options set by a check box or a choice. */
export type ToggleKey = 'fitsOneWay' | 'blind' | 'start' | 'ends' | 'through' | 'pattern';

/** The label of a size field (a key of the panel's `labels` strings). */
export type LabelKey =
  | NumberKey
  | 'fingers'
  | 'tabs'
  | 'dowels'
  | 'bolts'
  | 'keys'
  | 'wedge'
  | 'kerfDepth'
  | 'tails'
  | 'tailLength'
  | 'bolt'
  | 'boltLength'
  | 'keyLength'
  | 'knobs'
  | 'knobSize'
  | 'band'
  | 'web'
  | 'panelAngle'
  | 'neck'
  | 'head'
  | 'bendRadius'
  | 'bendAngle'
  | 'depthA'
  | 'cams'
  | 'camDiameter'
  | 'camReach'
  | 'screws'
  | 'screw';

export interface NumberSpec {
  key: NumberKey;
  /** Its label, when not the one named after the key. */
  label?: LabelKey;
  /** A length in the job unit, a whole count or an angle in degrees. */
  unit: 'length' | 'count' | 'angle';
  /** Shown in the panel; the rest are under More. */
  essential?: boolean;
}

export interface KindInfo {
  kind: JointKind;
  /** What the second click picks: an edge, a place on the face, or
   * nothing (a pattern in one part). */
  target: 'edge' | 'face' | 'none';
  numbers: readonly NumberSpec[];
  toggles: readonly ToggleKey[];
  /** The places clicked on the edges set where the joint goes (atA, atB). */
  usesPlace?: boolean;
}

/**
 * The joints the panel offers, in gallery order: box corners first, then
 * the T joints (one part standing on another's face), then joints edge to
 * edge or crossing, and last the pattern in one part.
 */
export const JOINT_KINDS: readonly KindInfo[] = [
  {
    kind: 'finger',
    target: 'edge',
    numbers: [
      { key: 'count', unit: 'count', essential: true, label: 'fingers' },
      { key: 'depth', unit: 'length' },
    ],
    toggles: ['fitsOneWay', 'start', 'blind'],
  },
  {
    kind: 'cornerDovetail',
    target: 'edge',
    numbers: [
      { key: 'count', unit: 'count', essential: true, label: 'tails' },
      { key: 'angle', unit: 'angle' },
    ],
    toggles: [],
  },
  {
    kind: 'rabbet',
    target: 'edge',
    numbers: [{ key: 'depth', unit: 'length', essential: true }],
    toggles: [],
  },
  {
    kind: 'vJoint',
    target: 'edge',
    numbers: [
      { key: 'angle', unit: 'angle', essential: true, label: 'panelAngle' },
      { key: 'count', unit: 'count', label: 'fingers' },
    ],
    toggles: ['fitsOneWay', 'start'],
  },
  {
    kind: 'dado',
    target: 'face',
    numbers: [
      { key: 'depth', unit: 'length', essential: true },
      { key: 'stop', unit: 'length' },
    ],
    toggles: [],
  },
  {
    kind: 'tabSlot',
    target: 'face',
    numbers: [
      { key: 'count', unit: 'count', essential: true, label: 'tabs' },
      { key: 'width', unit: 'length' },
      { key: 'proud', unit: 'length' },
    ],
    toggles: ['fitsOneWay', 'ends'],
  },
  {
    kind: 'mortiseTenon',
    target: 'face',
    numbers: [
      { key: 'width', unit: 'length', essential: true },
      { key: 'depth', unit: 'length' },
    ],
    toggles: ['blind'],
  },
  {
    kind: 'wedgedTenon',
    target: 'face',
    numbers: [
      { key: 'width', unit: 'length', essential: true },
      { key: 'proud', unit: 'length' },
      { key: 'length', unit: 'length', label: 'wedge' },
    ],
    toggles: [],
  },
  {
    kind: 'kerfTenon',
    target: 'face',
    numbers: [
      { key: 'width', unit: 'length', essential: true },
      { key: 'depth', unit: 'length', label: 'kerfDepth' },
    ],
    toggles: [],
  },
  {
    kind: 'dowel',
    target: 'face',
    numbers: [
      { key: 'count', unit: 'count', essential: true, label: 'dowels' },
      { key: 'diameter', unit: 'length' },
      { key: 'inset', unit: 'length' },
      { key: 'depth', unit: 'length' },
    ],
    toggles: ['through'],
  },
  {
    kind: 'captiveNut',
    target: 'face',
    numbers: [
      { key: 'count', unit: 'count', essential: true, label: 'bolts' },
      { key: 'diameter', unit: 'length', label: 'bolt' },
      { key: 'length', unit: 'length', label: 'boltLength' },
      { key: 'nutWidth', unit: 'length' },
      { key: 'nutThickness', unit: 'length' },
      { key: 'inset', unit: 'length' },
    ],
    toggles: [],
  },
  {
    kind: 'twistLock',
    target: 'face',
    numbers: [
      { key: 'width', unit: 'length', essential: true, label: 'neck' },
      { key: 'proud', unit: 'length', label: 'head' },
    ],
    toggles: [],
  },
  {
    kind: 'camLock',
    target: 'face',
    numbers: [
      { key: 'count', unit: 'count', essential: true, label: 'cams' },
      { key: 'diameter', unit: 'length', label: 'camDiameter' },
      { key: 'length', unit: 'length', label: 'camReach' },
      { key: 'depth', unit: 'length' },
      { key: 'inset', unit: 'length' },
    ],
    toggles: [],
  },
  {
    kind: 'groove',
    target: 'face',
    numbers: [
      { key: 'depth', unit: 'length', essential: true },
      { key: 'stop', unit: 'length' },
    ],
    toggles: [],
  },
  {
    kind: 'screws',
    target: 'face',
    numbers: [
      { key: 'count', unit: 'count', essential: true, label: 'screws' },
      { key: 'diameter', unit: 'length', label: 'screw' },
      { key: 'inset', unit: 'length' },
    ],
    toggles: [],
  },
  {
    kind: 'dovetail',
    target: 'edge',
    numbers: [
      { key: 'count', unit: 'count', essential: true, label: 'tails' },
      { key: 'depth', unit: 'length', label: 'tailLength' },
      { key: 'angle', unit: 'angle' },
    ],
    toggles: [],
  },
  {
    kind: 'butterfly',
    target: 'edge',
    numbers: [
      { key: 'count', unit: 'count', essential: true, label: 'keys' },
      { key: 'length', unit: 'length', label: 'keyLength' },
      { key: 'width', unit: 'length' },
      { key: 'depth', unit: 'length' },
    ],
    toggles: [],
  },
  {
    kind: 'puzzle',
    target: 'edge',
    numbers: [
      { key: 'count', unit: 'count', essential: true, label: 'knobs' },
      { key: 'width', unit: 'length', label: 'knobSize' },
    ],
    toggles: [],
  },
  {
    kind: 'crossLap',
    target: 'edge',
    numbers: [
      { key: 'detent', unit: 'length' },
      { key: 'atA', unit: 'length' },
      { key: 'atB', unit: 'length' },
      { key: 'depth', unit: 'length', label: 'depthA' },
      { key: 'depthB', unit: 'length' },
    ],
    toggles: [],
    usesPlace: true,
  },
  {
    kind: 'halfLap',
    target: 'edge',
    numbers: [{ key: 'depth', unit: 'length' }],
    toggles: [],
  },
  {
    kind: 'livingHinge',
    target: 'none',
    numbers: [
      { key: 'radius', unit: 'length', essential: true, label: 'bendRadius' },
      { key: 'angle', unit: 'angle', essential: true, label: 'bendAngle' },
      { key: 'width', unit: 'length', label: 'band' },
      { key: 'depth', unit: 'length', label: 'web' },
      { key: 'atA', unit: 'length' },
    ],
    toggles: ['pattern'],
    usesPlace: true,
  },
];

/** The clearance a side each named fit gives, mm. */
export const FIT_CLEARANCE_MM: Record<Exclude<Fit, 'custom'>, number> = {
  slip: 0.2,
  press: 0.1,
  glue: 0.15,
  knockDown: 0.25,
};

/** A fit's name with its clearance a side in the job unit. */
export function fitLabel(fit: Fit, units: Units): string {
  const fits = jointsStrings.joints.fits;
  if (fit === 'custom') {
    return fits.custom;
  }
  return fits[fit](`${lengthText(FIT_CLEARANCE_MM[fit], units)} ${unitSuffix(units)}`);
}

/**
 * The one line at the top of the panel: what the last action did, until
 * the next pick, else what to click next.
 */
export function shownLine(notice: string | null, hint: string): string {
  return notice ?? hint;
}

export function kindInfo(kind: JointKind): KindInfo {
  return JOINT_KINDS.find((k) => k.kind === kind) ?? JOINT_KINDS[0];
}

/** Part B as picked. */
export type PickedB =
  | { kind: 'edge'; edge: PickedEdge }
  | { kind: 'face'; edge: PickedEdge; offset: number; along: number };

export interface JointFormState {
  kind: JointKind;
  a: PickedEdge | null;
  b: PickedB | null;
  /** The joint being edited, or null for a new one. */
  editing: number | null;
  /** The panels of a box being made from the selection, or null. */
  box: number[] | null;
  numbers: Partial<Record<NumberKey, string>>;
  fitsOneWay: boolean;
  blind: boolean;
  start: 'finger' | 'gap';
  ends: 'square' | 'rounded';
  /** Pegs right through instead of blind dowels. */
  through: boolean;
  /** A living hinge's slots. */
  pattern: HingePattern;
  makeToolpaths: boolean;
}

export function initialJointState(): JointFormState {
  return {
    kind: 'finger',
    a: null,
    b: null,
    editing: null,
    box: null,
    numbers: {},
    fitsOneWay: false,
    blind: false,
    start: 'finger',
    ends: 'square',
    through: false,
    pattern: 'straight',
    makeToolpaths: false,
  };
}

/** Length fields follow the job unit. */
export function convertJointUnits(state: JointFormState, from: Units, to: Units): JointFormState {
  if (from === to) {
    return state;
  }
  const numbers: Partial<Record<NumberKey, string>> = { ...state.numbers };
  for (const spec of kindInfo(state.kind).numbers) {
    const text = numbers[spec.key];
    if (spec.unit !== 'length' || text === undefined) {
      continue;
    }
    const mm = lengthValue(text, from);
    if (mm !== null) {
      numbers[spec.key] = lengthText(mm, to);
    }
  }
  return { ...state, numbers };
}

export type Stage = 'pickA' | 'pickB' | 'ready' | 'box';

export function stage(state: JointFormState): Stage {
  if (state.box !== null) {
    return 'box';
  }
  if (state.a === null) {
    return 'pickA';
  }
  if (kindInfo(state.kind).target === 'none') {
    return 'ready';
  }
  return state.b === null ? 'pickB' : 'ready';
}

/** Esc: the last pick goes, then the first. */
export function stepBack(state: JointFormState): JointFormState {
  if (state.box !== null) {
    return { ...state, box: null };
  }
  if (state.b !== null) {
    return { ...state, b: null };
  }
  if (state.a !== null) {
    return { ...state, a: null, editing: null };
  }
  return state;
}

/** A new kind keeps the picks that still suit it. */
export function withKind(state: JointFormState, kind: JointKind): JointFormState {
  const target = kindInfo(kind).target;
  const b = state.b !== null && state.b.kind !== target ? null : state.b;
  if (target === 'none') {
    return { ...state, kind, b: null, numbers: {} };
  }
  return { ...state, kind, b, numbers: {} };
}

/** The reason a second pick cannot be used, or null. */
export function secondPickProblem(state: JointFormState, vector: number): string | null {
  if (state.a !== null && state.a.vector === vector) {
    return 'samePart';
  }
  return null;
}

export interface ParamsResult {
  params: JointParams;
  errors: Partial<Record<NumberKey, string>>;
}

/** The joint's own sizes in millimetres; empty fields are left to the kernel. */
export function paramsOf(state: JointFormState, units: Units): ParamsResult {
  const info = kindInfo(state.kind);
  const params: JointParams = {};
  const errors: Partial<Record<NumberKey, string>> = {};
  for (const spec of info.numbers) {
    const text = (state.numbers[spec.key] ?? '').trim();
    if (text === '') {
      continue;
    }
    const value = spec.unit === 'length' ? lengthValue(text, units) : numberValue(text);
    if (value === null || !Number.isFinite(value) || value <= 0) {
      errors[spec.key] = 'positive';
      continue;
    }
    if (spec.unit === 'count' && !Number.isInteger(value)) {
      errors[spec.key] = 'whole';
      continue;
    }
    params[spec.key] = value;
  }
  if (info.toggles.includes('fitsOneWay') && state.fitsOneWay) {
    params.bias = FITS_ONE_WAY_BIAS;
  }
  if (info.toggles.includes('blind') && state.blind) {
    params.blind = true;
  }
  if (info.toggles.includes('start') && state.start === 'gap') {
    params.start = 'gap';
  }
  if (info.toggles.includes('ends') && state.ends === 'rounded') {
    params.ends = 'rounded';
  }
  if (info.toggles.includes('through') && state.through) {
    params.blind = false;
  }
  if (info.toggles.includes('pattern') && state.pattern !== 'straight') {
    params.pattern = state.pattern;
  }
  return { params, errors };
}

/** How much wider the first finger or tab is for Fits one way only. */
export const FITS_ONE_WAY_BIAS = 0.3;

function edgeLine(edge: PickedEdge): EdgeLine {
  return { vector: edge.vector, from: edge.from, to: edge.to };
}

/** The joint to preview or create, or null until both picks are in. */
export function specOf(state: JointFormState, units: Units): JointSpec | null {
  if (state.a === null) {
    return null;
  }
  const info = kindInfo(state.kind);
  if (info.target === 'none') {
    // A pattern in one part: its own edge stands for the second pick.
    const { params, errors } = paramsOf(state, units);
    if (Object.keys(errors).length > 0) {
      return null;
    }
    if (info.usesPlace === true) {
      params.atA ??= state.a.at;
    }
    const own = edgeLine(state.a);
    return { kind: state.kind, a: own, b: { kind: 'edge', ...own }, params };
  }
  if (state.b === null) {
    return null;
  }
  const { params, errors } = paramsOf(state, units);
  if (Object.keys(errors).length > 0) {
    return null;
  }
  const b: Target =
    state.b.kind === 'edge'
      ? { kind: 'edge', ...edgeLine(state.b.edge) }
      : {
          kind: 'face',
          edge: edgeLine(state.b.edge),
          offset: state.b.offset,
          along: state.b.along,
        };
  if (info.usesPlace === true) {
    params.atA ??= state.a.at;
    params.atB ??= state.b.edge.at;
  }
  return { kind: state.kind, a: edgeLine(state.a), b, params };
}

/** A picked edge made from a stored joint's edge line. */
function pickedOf(edge: EdgeLine, joint: number): PickedEdge {
  const length = Math.hypot(edge.to[0] - edge.from[0], edge.to[1] - edge.from[1]);
  return { ...edge, length, sheet: 0, name: null, thickness: 0, joint };
}

/** The panel set up to edit a stored joint. */
export function editState(row: JointRow, units: Units, base: JointFormState): JointFormState {
  const info = kindInfo(row.kind);
  const numbers: Partial<Record<NumberKey, string>> = {};
  for (const spec of info.numbers) {
    const value = row.params[spec.key];
    if (typeof value === 'number') {
      numbers[spec.key] = spec.unit === 'length' ? lengthText(value, units) : String(value);
    }
  }
  const b: PickedB =
    row.b.kind === 'edge'
      ? { kind: 'edge', edge: pickedOf(row.b, row.id) }
      : {
          kind: 'face',
          edge: pickedOf(row.b.edge, row.id),
          offset: row.b.offset,
          along: row.b.along,
        };
  return {
    ...base,
    kind: row.kind,
    a: pickedOf(row.a, row.id),
    b,
    editing: row.id,
    box: null,
    numbers,
    fitsOneWay: (row.params.bias ?? 0) > 0,
    blind: row.params.blind === true,
    start: row.params.start ?? 'finger',
    ends: row.params.ends ?? 'square',
    through: row.params.blind === false,
    pattern: row.params.pattern ?? 'straight',
  };
}

/** The face pick moved to `offset` in from B's edge (millimetres). */
export function withOffset(state: JointFormState, offset: number): JointFormState {
  if (state.b === null || state.b.kind !== 'face' || !(offset >= 0)) {
    return state;
  }
  return { ...state, b: { ...state.b, offset } };
}

/** After Create: ready for the next joint of the same kind. */
export function afterCreate(state: JointFormState): JointFormState {
  return { ...state, a: null, b: null, editing: null, box: null };
}

/** The joints whose parts include `vector`. */
export function jointsOf(rows: readonly JointRow[], vector: number): JointRow[] {
  return rows.filter((row) => row.parts.includes(vector));
}

/**
 * A kernel message as a sentence: capitalized, ending in a period.
 */
export function sentence(message: string): string {
  const text = message.trim();
  if (text === '') {
    return text;
  }
  const first = text.charAt(0).toUpperCase() + text.slice(1);
  return /[.!?]$/.test(first) ? first : `${first}.`;
}
