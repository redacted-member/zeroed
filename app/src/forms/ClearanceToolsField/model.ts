// Pure state of the Use Clearance Tools list shared by the V-Groove and
// VGroove Inlay forms: the tools in cutting order, each with its own
// options (strategy and raster angle only count for the first) and, for a
// later V shaped tool, Corner Sharpen.

import type { ClearanceOptions, Tool, VGrooveClearanceTool } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import {
  clearanceStateFrom,
  clearanceValues,
  convertClearance,
  initialClearance,
  validateClearance,
  type ClearanceErrors,
  type ClearanceState,
} from '../../lib/clearanceFields';
import { strings } from '../../lib/formStrings';

export interface ClearanceEntry {
  tool: Tool;
  cornerSharpen: boolean;
  clearance: ClearanceState;
}

/** Whether a tool can sharpen corners: a V-bit or an engraving tool. */
export function canSharpen(tool: Tool): boolean {
  return tool.toolType.kind === 'vBit' || tool.toolType.kind === 'engravingTool';
}

/**
 * The entries for the tool list after the list changed: a tool kept (the
 * same object, or its edited copy in the same row) keeps its options, a
 * new one starts with the first tool's.
 */
export function syncEntries(
  entries: readonly ClearanceEntry[],
  tools: readonly Tool[],
  units: Units,
): ClearanceEntry[] {
  const fresh = entries[0]?.clearance ?? initialClearance(units);
  return tools.map((tool, index) => {
    const same = entries.find((entry) => entry.tool === tool);
    if (same !== undefined) {
      return same;
    }
    const row = entries[index];
    if (row !== undefined && row.tool.id === tool.id && tools.length === entries.length) {
      return { ...row, tool };
    }
    return { tool, cornerSharpen: false, clearance: fresh };
  });
}

/** The entries of a saved list; tools without their own options take `shared`. */
export function entriesFrom(
  list: readonly VGrooveClearanceTool[],
  shared: ClearanceOptions | undefined,
  units: Units,
): ClearanceEntry[] {
  return list.map((entry) => ({
    tool: entry.tool,
    cornerSharpen: entry.cornerSharpen,
    clearance: clearanceStateFrom(entry.options ?? shared, units),
  }));
}

export function convertEntries(
  entries: readonly ClearanceEntry[],
  from: Units,
  to: Units,
): ClearanceEntry[] {
  return entries.map((entry) => ({
    ...entry,
    clearance: convertClearance(entry.clearance, from, to),
  }));
}

/** The first tool's option errors or those of the first later tool with one. */
export function validateEntries(
  entries: readonly ClearanceEntry[],
  units: Units,
): { index: number; errors: ClearanceErrors } | null {
  for (const [index, entry] of entries.entries()) {
    const errors = validateClearance(entry.clearance, units);
    if (Object.keys(errors).length > 0) {
      return { index, errors };
    }
  }
  return null;
}

/**
 * The kernel's list: each tool with its options; the later tools take the
 * first tool's strategy and raster angle, and only a later V shaped tool
 * can sharpen corners.
 */
export function kernelEntries(
  entries: readonly ClearanceEntry[],
  units: Units,
  sharpen = true,
): VGrooveClearanceTool[] {
  const first = entries[0];
  return entries.map((entry, index) => {
    const own = clearanceValues(entry.clearance, units);
    const options =
      first === undefined || index === 0
        ? own
        : {
            ...own,
            strategy: first.clearance.strategy,
            rasterAngle: clearanceValues(first.clearance, units).rasterAngle,
          };
    return {
      tool: entry.tool,
      cornerSharpen: sharpen && index > 0 && entry.cornerSharpen && canSharpen(entry.tool),
      options,
    };
  });
}

/** The shared options older kernels read: the first tool's. */
export function sharedOptions(entries: readonly ClearanceEntry[], units: Units): ClearanceOptions {
  const first = entries[0];
  return clearanceValues(first?.clearance ?? initialClearance(units), units);
}

/** A problem with the first clearance tool that does not stop calculating. */
export function firstToolWarning(entries: readonly ClearanceEntry[]): string | undefined {
  const first = entries[0];
  return first !== undefined && first.tool.toolType.kind === 'vBit'
    ? strings.toolpaths.vgroove.clearanceV
    : undefined;
}
