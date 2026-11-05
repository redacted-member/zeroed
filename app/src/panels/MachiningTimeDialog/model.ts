// Pure parts of the Machining Time Estimate dialog: the visible toolpaths'
// times and total, and the scale factor as typed.

import type { ToolpathSummary } from '../../bridge/cam';
import type { MachinePrefs } from '../../bridge/prefs';
import { fieldError, numberValue } from '../../lib/formFields';

export interface TimeRow {
  id: number;
  name: string;
  seconds: number;
}

/** The visible toolpaths in list order with their estimates, and the total. */
export function timeRows(toolpaths: readonly ToolpathSummary[]): {
  rows: TimeRow[];
  total: number;
} {
  const rows = toolpaths
    .filter((t) => t.visible)
    .map((t) => ({ id: t.id, name: t.name, seconds: t.estimatedSeconds }));
  return { rows, total: rows.reduce((sum, row) => sum + row.seconds, 0) };
}

/** The scale factor typed, or an error: 0.1 to 10 as the kernel accepts. */
export function scaleValue(text: string): { value: number | null; error?: string } {
  const value = numberValue(text);
  const error = fieldError(value, { min: 0.1, max: 10 });
  return error === undefined ? { value } : { value: null, error };
}

/** The active machine's rapid rate (mm/min) and whether it has accelerations. */
export function machineTiming(
  machines: MachinePrefs,
): { name: string; rapidRate: number; accelerated: boolean } | null {
  const machine = machines.list.find((m) => m.name === machines.active);
  if (machine === undefined) {
    return null;
  }
  const accel = machine.acceleration ?? [0, 0, 0];
  return {
    name: machine.name,
    rapidRate: machine.rapidRate ?? 3000,
    accelerated: accel.every((a) => a > 0),
  };
}
