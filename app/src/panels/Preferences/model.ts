// Form state of the Preferences dialog: a working copy of the preferences
// with number fields kept as typed text until they are applied.

import type { GeneralPrefs, Preferences, PrefsPatch } from '../../bridge/prefs';
import type { Units } from '../../bridge/types';
import {
  MAX_AUTOSAVE_MINUTES,
  MAX_GEOMETRY_FIX_SECONDS,
  MAX_RECENT_FILES,
  MAX_UNDO_LIMIT,
} from '../../lib/prefs';

const MM_PER_INCH = 25.4;

export type PrefsSection = 'general' | 'view2d' | 'view3d' | 'toolpaths' | 'confirmations';

export const PREFS_SECTIONS: readonly PrefsSection[] = [
  'general',
  'view2d',
  'view3d',
  'toolpaths',
  'confirmations',
];

export type NumberField =
  | 'undoLimit'
  | 'autosaveMinutes'
  | 'recentFilesLength'
  | 'snapRadiusPx'
  | 'gridSpacing'
  | 'tolerance'
  | 'tolerance3d'
  | 'vgrooveTolerance'
  | 'geometryFixSeconds';

interface NumberRule {
  min: number;
  max: number;
  whole: boolean;
  /** A length: stored and limited in millimeters, shown in the default units. */
  length?: boolean;
  /** Where the field lives in the preferences. */
  read: (prefs: Preferences) => number;
  write: (prefs: Preferences, value: number) => void;
}

export const NUMBER_RULES: Record<NumberField, NumberRule> = {
  undoLimit: {
    min: 1,
    max: MAX_UNDO_LIMIT,
    whole: true,
    read: (p) => p.general.undoLimit,
    write: (p, v) => {
      p.general.undoLimit = v;
    },
  },
  autosaveMinutes: {
    min: 0,
    max: MAX_AUTOSAVE_MINUTES,
    whole: true,
    read: (p) => p.general.autosaveMinutes,
    write: (p, v) => {
      p.general.autosaveMinutes = v;
    },
  },
  recentFilesLength: {
    min: 0,
    max: MAX_RECENT_FILES,
    whole: true,
    read: (p) => p.general.recentFilesLength,
    write: (p, v) => {
      p.general.recentFilesLength = v;
    },
  },
  snapRadiusPx: {
    min: 1,
    max: 50,
    whole: true,
    read: (p) => p.view2d.snapRadiusPx,
    write: (p, v) => {
      p.view2d.snapRadiusPx = v;
    },
  },
  gridSpacing: {
    min: 0.001,
    max: 10000,
    whole: false,
    length: true,
    read: (p) => p.view2d.gridSpacing,
    write: (p, v) => {
      p.view2d.gridSpacing = v;
    },
  },
  tolerance: {
    min: 0.0001,
    max: 1,
    whole: false,
    length: true,
    read: (p) => p.toolpaths.tolerance,
    write: (p, v) => {
      p.toolpaths.tolerance = v;
    },
  },
  tolerance3d: {
    min: 0.0001,
    max: 1,
    whole: false,
    length: true,
    read: (p) => p.toolpaths.tolerance3d,
    write: (p, v) => {
      p.toolpaths.tolerance3d = v;
    },
  },
  vgrooveTolerance: {
    min: 0.0001,
    max: 1,
    whole: false,
    length: true,
    read: (p) => p.toolpaths.vgrooveTolerance,
    write: (p, v) => {
      p.toolpaths.vgrooveTolerance = v;
    },
  },
  geometryFixSeconds: {
    min: 0,
    max: MAX_GEOMETRY_FIX_SECONDS,
    whole: true,
    read: (p) => p.toolpaths.geometryFixSeconds,
    write: (p, v) => {
      p.toolpaths.geometryFixSeconds = v;
    },
  },
};

const NUMBER_FIELDS = Object.keys(NUMBER_RULES) as NumberField[];

export interface PrefsDraft {
  prefs: Preferences;
  numbers: Record<NumberField, string>;
  /** The user asked to empty the recent files list. */
  clearRecent: boolean;
}

/** Whether a field is a length shown in the default units. */
export function isLengthField(field: NumberField): boolean {
  return NUMBER_RULES[field].length === true;
}

/** A stored value as the field's text in `units`. */
function shownNumber(field: NumberField, value: number, units: Units): string {
  return isLengthField(field) && units === 'inch'
    ? String(Number((value / MM_PER_INCH).toPrecision(4)))
    : String(value);
}

/** A field's limits as the user reads them. */
export function shownRange(field: NumberField, units: Units): { min: string; max: string } {
  const rule = NUMBER_RULES[field];
  return { min: shownNumber(field, rule.min, units), max: shownNumber(field, rule.max, units) };
}

/** A draft showing `prefs`. */
export function draftFrom(prefs: Preferences): PrefsDraft {
  const copy = structuredClone(prefs);
  const units = copy.general.defaultUnits;
  const numbers = Object.fromEntries(
    NUMBER_FIELDS.map((field) => [
      field,
      shownNumber(field, NUMBER_RULES[field].read(copy), units),
    ]),
  ) as Record<NumberField, string>;
  return { prefs: copy, numbers, clearRecent: false };
}

/** The value a field's text stands for, in the stored unit. */
function storedNumber(field: NumberField, text: string, units: Units): number {
  const value = Number(text.trim());
  return isLengthField(field) && units === 'inch' ? value * MM_PER_INCH : value;
}

/** The draft in new default units, with its length fields converted. */
export function withDefaultUnits(draft: PrefsDraft, units: Units): PrefsDraft {
  const from = draft.prefs.general.defaultUnits;
  const numbers = { ...draft.numbers };
  for (const field of NUMBER_FIELDS.filter(isLengthField)) {
    const stored = NUMBER_RULES[field].read(draft.prefs);
    const untouched = numbers[field].trim() === shownNumber(field, stored, from);
    if (untouched) {
      numbers[field] = shownNumber(field, stored, units);
    } else if (numberProblem(field, numbers[field], from) === null) {
      numbers[field] = shownNumber(field, storedNumber(field, numbers[field], from), units);
    }
  }
  return { ...editDraft(draft, 'general', { defaultUnits: units }), numbers };
}

/** The draft with one section of the preferences changed. */
export function editDraft<S extends Exclude<keyof Preferences, 'version'>>(
  draft: PrefsDraft,
  section: S,
  patch: Partial<Preferences[S]>,
): PrefsDraft {
  return {
    ...draft,
    prefs: { ...draft.prefs, [section]: { ...draft.prefs[section], ...patch } },
  };
}

/** Why the text of a number field is not acceptable, or null. */
export function numberProblem(
  field: NumberField,
  text: string,
  units: Units = 'mm',
): string | null {
  const rule = NUMBER_RULES[field];
  const value = storedNumber(field, text, units);
  if (text.trim() === '' || !Number.isFinite(value)) {
    return 'number';
  }
  if (rule.whole && !Number.isInteger(value)) {
    return 'whole';
  }
  if (value < rule.min || value > rule.max) {
    return 'range';
  }
  return null;
}

/** Every number field that is not acceptable, with the reason. */
export function draftProblems(draft: PrefsDraft): Partial<Record<NumberField, string>> {
  const problems: Partial<Record<NumberField, string>> = {};
  for (const field of NUMBER_FIELDS) {
    const problem = numberProblem(field, draft.numbers[field], draft.prefs.general.defaultUnits);
    if (problem !== null) {
      problems[field] = problem;
    }
  }
  return problems;
}

/** The prefs.set patch for an acceptable draft, or null when it is not. */
export function draftPatch(draft: PrefsDraft): PrefsPatch | null {
  if (Object.keys(draftProblems(draft)).length > 0) {
    return null;
  }
  const prefs = structuredClone(draft.prefs);
  const units = prefs.general.defaultUnits;
  for (const field of NUMBER_FIELDS) {
    // A length left alone keeps its exact stored value, not the rounded text.
    const stored = NUMBER_RULES[field].read(prefs);
    if (draft.numbers[field].trim() !== shownNumber(field, stored, units)) {
      NUMBER_RULES[field].write(prefs, storedNumber(field, draft.numbers[field], units));
    }
  }
  // The recent files list is the File menu's, not the dialog's, unless the
  // user emptied it here.
  const general: Partial<GeneralPrefs> = { ...prefs.general };
  delete general.recentFiles;
  if (draft.clearRecent) {
    general.recentFiles = [];
  }
  return {
    general,
    view2d: prefs.view2d,
    view3d: prefs.view3d,
    toolpaths: prefs.toolpaths,
    confirmations: prefs.confirmations,
    window: prefs.window,
  };
}
