// Form state of the Preferences dialog: a working copy of the preferences
// with number fields kept as typed text until they are applied.

import type { GeneralPrefs, Preferences, PrefsPatch } from '../../bridge/prefs';
import {
  MAX_AUTOSAVE_MINUTES,
  MAX_GEOMETRY_FIX_SECONDS,
  MAX_RECENT_FILES,
  MAX_UNDO_LIMIT,
} from '../../lib/prefs';

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
    read: (p) => p.view2d.gridSpacing,
    write: (p, v) => {
      p.view2d.gridSpacing = v;
    },
  },
  tolerance: {
    min: 0.0001,
    max: 1,
    whole: false,
    read: (p) => p.toolpaths.tolerance,
    write: (p, v) => {
      p.toolpaths.tolerance = v;
    },
  },
  tolerance3d: {
    min: 0.0001,
    max: 1,
    whole: false,
    read: (p) => p.toolpaths.tolerance3d,
    write: (p, v) => {
      p.toolpaths.tolerance3d = v;
    },
  },
  vgrooveTolerance: {
    min: 0.0001,
    max: 1,
    whole: false,
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

/** A draft showing `prefs`. */
export function draftFrom(prefs: Preferences): PrefsDraft {
  const copy = structuredClone(prefs);
  const numbers = Object.fromEntries(
    NUMBER_FIELDS.map((field) => [field, String(NUMBER_RULES[field].read(copy))]),
  ) as Record<NumberField, string>;
  return { prefs: copy, numbers, clearRecent: false };
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
export function numberProblem(field: NumberField, text: string): string | null {
  const rule = NUMBER_RULES[field];
  const value = Number(text.trim());
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
    const problem = numberProblem(field, draft.numbers[field]);
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
  for (const field of NUMBER_FIELDS) {
    NUMBER_RULES[field].write(prefs, Number(draft.numbers[field].trim()));
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
