// Pure state of the Machine Configuration dialog: an editable copy of the
// machine list, its checks, the preferences it saves and the matching
// changes to the machine names the tool database keeps cutting data under.

import type { MachineConfig, MachinePrefs } from '../../bridge/prefs';
import { rotaryWiring } from '../../bridge/prefs';
import type { Units } from '../../bridge/types';
import { fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { machineSetupStrings } from '../../lib/strings/machineSetup';

const text = machineSetupStrings.machineConfig;

/** Largest travel on any axis, millimetres (the kernel's limit). */
export const MAX_TRAVEL_MM = 100_000;

export type TravelField = 'bedX' | 'bedY' | 'bedZ';
export const TRAVEL_FIELDS: readonly TravelField[] = ['bedX', 'bedY', 'bedZ'];

/** Time estimate fields: the rapid rate per minute, the accelerations in
 * the chosen unit and the rotary axis in degrees per second squared. */
export type TimingField = 'rapidRate' | 'accelX' | 'accelY' | 'accelZ' | 'rotaryAccel';
export const TIMING_FIELDS: readonly TimingField[] = [
  'rapidRate',
  'accelX',
  'accelY',
  'accelZ',
  'rotaryAccel',
];

/** Units the accelerations can be typed in. */
export const ACCEL_UNITS = [
  'mmPerSec2',
  'inchPerSec2',
  'mmPerMin2',
  'inchPerMin2',
  'mPerMin2',
  'ftPerMin2',
] as const;
export type AccelUnit = (typeof ACCEL_UNITS)[number];

/** Millimetres per second squared in one of each unit. */
const MM_PER_S2: Record<AccelUnit, number> = {
  mmPerSec2: 1,
  inchPerSec2: 25.4,
  mmPerMin2: 1 / 3600,
  inchPerMin2: 25.4 / 3600,
  mPerMin2: 1000 / 3600,
  ftPerMin2: 304.8 / 3600,
};

function isAccelUnit(value: string | undefined): value is AccelUnit {
  return (ACCEL_UNITS as readonly string[]).includes(value ?? '');
}

/** A number as typed, without float noise. */
function plainText(value: number): string {
  return String(Math.round(value * 1e6) / 1e6);
}

/** An acceleration in mm/s2 as typed in `unit`. */
export function accelText(mmPerS2: number, unit: AccelUnit): string {
  return plainText(mmPerS2 / MM_PER_S2[unit]);
}

/** A typed acceleration in mm/s2, or null when it is not a number. */
export function accelValue(text: string, unit: AccelUnit): number | null {
  const value = Number(text);
  return text.trim() === '' || !Number.isFinite(value) ? null : value * MM_PER_S2[unit];
}

/** Rapid rate a machine saved before it had one uses, mm/min. */
export const DEFAULT_RAPID_RATE = 3000;

export interface MachineDraft {
  /** Stable key for the list while names change. */
  key: number;
  /** The name when the dialog opened, null for a machine added since. */
  original: string | null;
  name: string;
  units: Units;
  /** Travel as typed, in the machine's units. */
  bedX: string;
  bedY: string;
  bedZ: string;
  /** Rapid rate as typed, machine units per minute. */
  rapidRate: string;
  /** Accelerations as typed, machine units per second squared. */
  accelX: string;
  accelY: string;
  accelZ: string;
  /** Rotary axis acceleration as typed, degrees per second squared. */
  rotaryAccel: string;
  accelUnits: AccelUnit;
  /** How the rotary axis is wired. */
  rotaryLetter: 'A' | 'B';
  rotaryUnits: 'degrees' | 'linear';
  rotaryReversed: boolean;
  posts: string[];
  defaultPost: string | null;
}

export interface MachinesDraft {
  /** Key of the active machine, or null when there are none. */
  active: number | null;
  machines: MachineDraft[];
  nextKey: number;
}

export type MachineProblems = Partial<Record<'name' | TravelField | TimingField, string>>;

function draftOf(machine: MachineConfig, key: number): MachineDraft {
  const wiring = rotaryWiring(machine);
  return {
    key,
    original: machine.name,
    name: machine.name,
    units: machine.units,
    bedX: lengthText(machine.bedX, machine.units),
    bedY: lengthText(machine.bedY, machine.units),
    bedZ: lengthText(machine.bedZ, machine.units),
    rapidRate: lengthText(machine.rapidRate ?? DEFAULT_RAPID_RATE, machine.units),
    ...accelDraft(machine),
    rotaryLetter: wiring.letter ?? 'A',
    rotaryUnits: wiring.units,
    rotaryReversed: wiring.reversed,
    posts: [...machine.posts],
    defaultPost: machine.defaultPost ?? null,
  };
}

/** The acceleration fields of a machine's draft, in its chosen unit. */
function accelDraft(
  machine: MachineConfig,
): Pick<MachineDraft, 'accelX' | 'accelY' | 'accelZ' | 'rotaryAccel' | 'accelUnits'> {
  const unit: AccelUnit = isAccelUnit(machine.accelerationUnits)
    ? machine.accelerationUnits
    : machine.units === 'inch'
      ? 'inchPerSec2'
      : 'mmPerSec2';
  const [x, y, z] = machine.acceleration ?? [0, 0, 0];
  return {
    accelX: accelText(x, unit),
    accelY: accelText(y, unit),
    accelZ: accelText(z, unit),
    rotaryAccel: plainText(machine.rotaryAcceleration ?? 0),
    accelUnits: unit,
  };
}

/** Switches the unit the accelerations are typed in, converting them. */
export function withAccelUnits(machine: MachineDraft, unit: AccelUnit): MachineDraft {
  const convert = (text: string) => {
    const value = accelValue(text, machine.accelUnits);
    return value === null ? text : accelText(value, unit);
  };
  return {
    ...machine,
    accelUnits: unit,
    accelX: convert(machine.accelX),
    accelY: convert(machine.accelY),
    accelZ: convert(machine.accelZ),
  };
}

/** The dialog state for the saved machines. */
export function draftFrom(prefs: MachinePrefs): MachinesDraft {
  const machines = prefs.list.map((machine, index) => draftOf(machine, index + 1));
  const active = machines.find((m) => m.name === prefs.active) ?? machines[0];
  return { active: active?.key ?? null, machines, nextKey: machines.length + 1 };
}

export function activeMachine(draft: MachinesDraft): MachineDraft | null {
  return draft.machines.find((m) => m.key === draft.active) ?? null;
}

/** The first "Machine N" not already taken. */
export function freeName(draft: MachinesDraft, base: string): string {
  const taken = new Set(draft.machines.map((m) => m.name.trim().toLowerCase()));
  if (!taken.has(base.toLowerCase())) {
    return base;
  }
  for (let n = 2; ; n += 1) {
    const name = `${base} ${n}`;
    if (!taken.has(name.toLowerCase())) {
      return name;
    }
  }
}

/** Adds a machine with default travel and makes it active. */
export function addMachine(draft: MachinesDraft, units: Units): MachinesDraft {
  const inch = units === 'inch';
  const machine = draftOf(
    {
      name: freeName(draft, text.newName),
      units,
      bedX: inch ? 609.6 : 600,
      bedY: inch ? 609.6 : 600,
      bedZ: inch ? 101.6 : 100,
      rapidRate: inch ? 3048 : DEFAULT_RAPID_RATE,
      posts: [],
    },
    draft.nextKey,
  );
  return {
    active: machine.key,
    machines: [...draft.machines, { ...machine, original: null }],
    nextKey: draft.nextKey + 1,
  };
}

/** Copies the active machine under a free name and makes the copy active. */
export function duplicateMachine(draft: MachinesDraft): MachinesDraft {
  const source = activeMachine(draft);
  if (source === null) {
    return draft;
  }
  const copy: MachineDraft = {
    ...source,
    key: draft.nextKey,
    original: null,
    name: freeName(draft, text.copyName(source.name.trim())),
    posts: [...source.posts],
  };
  return { active: copy.key, machines: [...draft.machines, copy], nextKey: draft.nextKey + 1 };
}

/** Removes the active machine; the one before it (or the first) becomes active. */
export function deleteMachine(draft: MachinesDraft): MachinesDraft {
  const index = draft.machines.findIndex((m) => m.key === draft.active);
  if (index < 0) {
    return draft;
  }
  const machines = draft.machines.filter((_, i) => i !== index);
  const next = machines[Math.max(0, index - 1)] ?? null;
  return { ...draft, machines, active: next?.key ?? null };
}

export function activate(draft: MachinesDraft, key: number): MachinesDraft {
  return draft.machines.some((m) => m.key === key) ? { ...draft, active: key } : draft;
}

/** Applies `change` to the active machine. */
export function editActive(
  draft: MachinesDraft,
  change: (machine: MachineDraft) => MachineDraft,
): MachinesDraft {
  return {
    ...draft,
    machines: draft.machines.map((m) => (m.key === draft.active ? change(m) : m)),
  };
}

/** Switches the machine's units, converting the typed travel. */
export function withUnits(machine: MachineDraft, units: Units): MachineDraft {
  if (units === machine.units) {
    return machine;
  }
  const convert = (value: string) => {
    const mm = lengthValue(value, machine.units);
    return mm === null ? value : lengthText(mm, units);
  };
  return {
    ...machine,
    units,
    bedX: convert(machine.bedX),
    bedY: convert(machine.bedY),
    bedZ: convert(machine.bedZ),
    rapidRate: convert(machine.rapidRate),
  };
}

/** Associates a post; the first one becomes the default. */
export function addPost(machine: MachineDraft, post: string): MachineDraft {
  if (machine.posts.includes(post)) {
    return machine;
  }
  return {
    ...machine,
    posts: [...machine.posts, post],
    defaultPost: machine.defaultPost ?? post,
  };
}

export function removePost(machine: MachineDraft, post: string): MachineDraft {
  const posts = machine.posts.filter((p) => p !== post);
  const defaultPost = machine.defaultPost === post ? (posts[0] ?? null) : machine.defaultPost;
  return { ...machine, posts, defaultPost };
}

/** What is wrong with one machine, by field. */
export function machineProblems(draft: MachinesDraft, machine: MachineDraft): MachineProblems {
  const problems: MachineProblems = {};
  const name = machine.name.trim();
  if (name === '') {
    problems.name = text.nameRequired;
  } else if (
    draft.machines.some(
      (other) =>
        other.key !== machine.key && other.name.trim().toLowerCase() === name.toLowerCase(),
    )
  ) {
    problems.name = text.nameTaken;
  }
  for (const field of TRAVEL_FIELDS) {
    const mm = lengthValue(machine[field], machine.units);
    const problem =
      fieldError(mm, { positive: true }) ??
      (mm !== null && mm > MAX_TRAVEL_MM ? text.travelTooLarge : undefined);
    if (problem !== undefined) {
      problems[field] = problem;
    }
  }
  for (const field of TIMING_FIELDS) {
    const value =
      field === 'rapidRate'
        ? lengthValue(machine[field], machine.units)
        : field === 'rotaryAccel'
          ? accelValue(machine[field], 'mmPerSec2')
          : accelValue(machine[field], machine.accelUnits);
    const problem = fieldError(value, field === 'rapidRate' ? { positive: true } : { min: 0 });
    if (problem !== undefined) {
      problems[field] = problem;
    }
  }
  return problems;
}

/** True when every machine is fine. */
export function draftValid(draft: MachinesDraft): boolean {
  return draft.machines.every((m) => Object.keys(machineProblems(draft, m)).length === 0);
}

/** The machines preferences the draft describes, or null while it has problems. */
export function draftPrefs(draft: MachinesDraft): MachinePrefs | null {
  if (!draftValid(draft)) {
    return null;
  }
  const list: MachineConfig[] = [];
  for (const machine of draft.machines) {
    // Micrometre rounding keeps an untouched inch machine at its saved size.
    const travel = (field: TravelField | 'rapidRate') => {
      const mm = lengthValue(machine[field], machine.units);
      return mm === null ? null : Math.round(mm * 1e6) / 1e6;
    };
    const bedX = travel('bedX');
    const bedY = travel('bedY');
    const bedZ = travel('bedZ');
    const rapidRate = travel('rapidRate');
    const ax = accelValue(machine.accelX, machine.accelUnits);
    const ay = accelValue(machine.accelY, machine.accelUnits);
    const az = accelValue(machine.accelZ, machine.accelUnits);
    const turn = accelValue(machine.rotaryAccel, 'mmPerSec2');
    if (
      bedX === null ||
      bedY === null ||
      bedZ === null ||
      rapidRate === null ||
      ax === null ||
      ay === null ||
      az === null ||
      turn === null
    ) {
      return null;
    }
    const config: MachineConfig = {
      name: machine.name.trim(),
      units: machine.units,
      bedX,
      bedY,
      bedZ,
      posts: [...machine.posts],
      rapidRate,
      acceleration: [ax, ay, az].map((a) => Math.round(a * 1e6) / 1e6) as [number, number, number],
      rotaryAcceleration: turn,
      rotary: {
        letter: machine.rotaryLetter,
        units: machine.rotaryUnits,
        reversed: machine.rotaryReversed,
      },
      accelerationUnits: machine.accelUnits,
    };
    if (machine.defaultPost !== null && machine.posts.includes(machine.defaultPost)) {
      config.defaultPost = machine.defaultPost;
    }
    list.push(config);
  }
  return { active: activeMachine(draft)?.name.trim() ?? '', list };
}

export interface ToolDbChanges {
  rename: Array<{ from: string; to: string }>;
  remove: string[];
  add: string[];
}

/**
 * How the tool database's machine names must change to follow the draft:
 * renamed machines carry their cutting data, deleted ones drop it, and
 * machines the database does not know yet are added.
 */
export function toolDbChanges(
  initial: readonly string[],
  draft: MachinesDraft,
  known: readonly string[],
): ToolDbChanges {
  const kept = new Set(draft.machines.map((m) => m.original).filter((n) => n !== null));
  const remove = initial.filter((name) => !kept.has(name) && known.includes(name));
  const rename: Array<{ from: string; to: string }> = [];
  const after = new Set(known.filter((name) => !remove.includes(name)));
  for (const machine of draft.machines) {
    const name = machine.name.trim();
    if (machine.original !== null && machine.original !== name && after.has(machine.original)) {
      rename.push({ from: machine.original, to: name });
      after.delete(machine.original);
      after.add(name);
    }
  }
  const add = draft.machines.map((m) => m.name.trim()).filter((name) => !after.has(name));
  return { rename, remove, add };
}

/** The post Save Toolpaths starts with: the active machine's default, else its first post. */
export function activeSavePost(prefs: MachinePrefs): string | null {
  const machine = prefs.list.find((m) => m.name === prefs.active);
  return machine?.defaultPost ?? machine?.posts[0] ?? null;
}
