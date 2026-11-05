// Pure state for the V-Groove Inlay Toolpath form: Pocket Depth, Glue Gap and
// Surface Gap, the V-bit, the clearance tools, the sheet the plug goes
// on and the plug's outer boundary. Calculate makes the pocket, then the
// plug from mirrored copies of the vectors on the Plug Destination Sheet.

import type { InlayParams, InlayPart, PlugBoundary, Tool } from '../../bridge/cam';
import type { Sheet } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import type { ClearanceErrors } from '../../lib/clearanceFields';
import { strings } from '../../lib/formStrings';
import {
  convertEntries,
  entriesFrom,
  kernelEntries,
  sharedOptions,
  validateEntries,
  type ClearanceEntry,
} from '../ClearanceToolsField/model';

const text = strings.toolpaths.inlay;

export interface InlayFormState {
  name: string;
  pocketDepth: string;
  glueGap: string;
  surfaceGap: string;
  plugBoundary: PlugBoundary;
  boundaryOffset: string;
  /** Use Clearance Tools is ticked. */
  useClearance: boolean;
  clearanceTools: ClearanceEntry[];
  /** Plug Destination Sheet; null for the sheet the vectors are on. */
  plugSheet: number | null;
  /** The kernel mirrors the plug itself: only toolpaths made before the plug
   * got its own mirrored vectors. */
  mirrorPlug: boolean;
}

export type InlayField = 'pocketDepth' | 'glueGap' | 'surfaceGap' | 'boundaryOffset';
export type InlayErrors = ClearanceErrors & Partial<Record<InlayField, string>>;

/** One toolpath to calculate: the part it cuts and its name. */
export interface InlayJob {
  part: InlayPart;
  name: string;
  params: InlayParams;
}

const LENGTHS: readonly InlayField[] = ['pocketDepth', 'glueGap', 'surfaceGap', 'boundaryOffset'];

export function initialInlayState(units: Units): InlayFormState {
  const inch = units === 'inch';
  return {
    name: '',
    pocketDepth: lengthText(inch ? 5.08 : 5, units),
    glueGap: lengthText(inch ? 0.254 : 0.2, units),
    surfaceGap: lengthText(inch ? 0.254 : 0.2, units),
    plugBoundary: 'sheetLimits',
    boundaryOffset: lengthText(inch ? 2.54 : 2, units),
    useClearance: false,
    clearanceTools: [],
    plugSheet: null,
    mirrorPlug: false,
  };
}

export function convertInlayUnits(state: InlayFormState, from: Units, to: Units): InlayFormState {
  return {
    ...convertLengthFields(state, LENGTHS, from, to),
    clearanceTools: convertEntries(state.clearanceTools, from, to),
  };
}

export function validateInlay(state: InlayFormState, units: Units): InlayErrors {
  const invalid = state.useClearance ? validateEntries(state.clearanceTools, units) : null;
  const errors: InlayErrors = invalid === null ? {} : { ...invalid.errors };
  const pocket = lengthValue(state.pocketDepth, units);
  const pocketError = fieldError(pocket, { positive: true });
  if (pocketError !== undefined) {
    errors.pocketDepth = pocketError;
  }
  const gap = lengthValue(state.glueGap, units);
  const gapError = fieldError(gap, { positive: true });
  if (gapError !== undefined) {
    errors.glueGap = gapError;
  } else if (gap !== null && pocket !== null && pocketError === undefined && gap >= pocket) {
    errors.glueGap = text.glueTooDeep;
  }
  const surfaceError = fieldError(lengthValue(state.surfaceGap, units), { positive: true });
  if (surfaceError !== undefined) {
    errors.surfaceGap = surfaceError;
  }
  if (state.plugBoundary === 'vectorOffset') {
    const offsetError = fieldError(lengthValue(state.boundaryOffset, units), { min: 0 });
    if (offsetError !== undefined) {
      errors.boundaryOffset = offsetError;
    }
  }
  return errors;
}

/** True for the tools whose cone can carve an inlay. */
export function isVShaped(tool: Tool): boolean {
  const kind = tool.toolType.kind;
  return kind === 'vBit' || kind === 'engravingTool';
}

export function inlayToolWarning(tool: Tool | null): string | undefined {
  return tool !== null && !isVShaped(tool) ? text.notV : undefined;
}

/** Kernel params for one part, or null while the form is not valid. */
export function inlayParams(
  state: InlayFormState,
  units: Units,
  part: InlayPart,
): InlayParams | null {
  if (Object.keys(validateInlay(state, units)).length > 0) {
    return null;
  }
  return {
    pocketDepth: lengthValue(state.pocketDepth, units) ?? 0,
    glueGap: lengthValue(state.glueGap, units) ?? 0,
    plugDepth: lengthValue(state.surfaceGap, units),
    plugBoundary: state.plugBoundary,
    boundaryExtra: lengthValue(state.boundaryOffset, units) ?? 0,
    // The plug's vectors are mirrored copies, so the kernel does not mirror
    // again; older plugs mirror their original vectors.
    mirrorPlug: state.mirrorPlug,
    boundaryOffset: null,
    // Passes of the V-bit's own pass depth.
    maxPassDepth: null,
    clearanceTool: null,
    clearanceOptions: sharedOptions(state.clearanceTools, units),
    clearanceTools: kernelEntries(state.useClearance ? state.clearanceTools : [], units, false),
    part,
  };
}

/**
 * The name the parts share: the typed name, or the first "VGroove Inlay N"
 * whose pocket and plug names are both free.
 */
export function inlayBaseName(typed: string, taken: readonly string[]): string {
  const trimmed = typed.trim();
  if (trimmed !== '') {
    return trimmed;
  }
  const used = new Set(taken);
  for (let n = 1; ; n += 1) {
    const base = `${text.defaultName} ${n}`;
    if (!used.has(text.pocketName(base)) && !used.has(text.plugName(base))) {
      return base;
    }
  }
}

/**
 * The toolpaths Calculate makes: the pocket, then the plug. When editing,
 * only the part being edited, under the name in the form.
 */
export function inlayJobs(
  state: InlayFormState,
  units: Units,
  taken: readonly string[],
  editing: InlayPart | null,
): InlayJob[] {
  const parts: InlayPart[] = editing !== null ? [editing] : ['female', 'male'];
  const base = inlayBaseName(state.name, taken);
  const jobs: InlayJob[] = [];
  for (const part of parts) {
    const params = inlayParams(state, units, part);
    if (params === null) {
      return [];
    }
    const name =
      editing !== null
        ? state.name.trim()
        : part === 'female'
          ? text.pocketName(base)
          : text.plugName(base);
    jobs.push({ part, name, params });
  }
  return jobs;
}

export function inlayStateFrom(params: InlayParams, name: string, units: Units): InlayFormState {
  const base = initialInlayState(units);
  const length = (value: number | null | undefined, fallback: string) =>
    value === null || value === undefined ? fallback : lengthText(value, units);
  const list = params.clearanceTools ?? [];
  const legacy = params.clearanceTool ?? null;
  const saved =
    list.length === 0 && legacy !== null ? [{ tool: legacy, cornerSharpen: false }] : list;
  const entries = entriesFrom(saved, params.clearanceOptions, units);
  return {
    name,
    pocketDepth: lengthText(params.pocketDepth, units),
    glueGap: lengthText(params.glueGap, units),
    // Older toolpaths cleared the plug waste as deep as the pocket.
    surfaceGap: length(params.plugDepth ?? params.pocketDepth, base.surfaceGap),
    plugBoundary: params.plugBoundary ?? 'vectorOffset',
    boundaryOffset: length(params.boundaryExtra, base.boundaryOffset),
    useClearance: entries.length > 0,
    clearanceTools: entries,
    plugSheet: null,
    mirrorPlug: params.mirrorPlug,
  };
}

/** The active sheet's id, or null before the sheets are read. */
export function activeSheetOf(sheets: readonly Sheet[]): number | null {
  return sheets.find((sheet) => sheet.active)?.id ?? null;
}

/** The sheet the plug goes on: the one picked, else the active sheet. */
export function plugSheetFor(state: InlayFormState, active: number | null): number | null {
  return state.plugSheet ?? active;
}
