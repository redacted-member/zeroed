// The object snap toggle of the 2D view toolbar: one button for the snaps
// Snap Settings (F4) sets one by one.

import type { SnapOptions } from '../../bridge/document';
import { strings } from '../../lib/strings';
import { commandTip } from '../../shell/Tooltip/model';

export const OBJECT_SNAPS: ReadonlyArray<keyof SnapOptions> = [
  'nodes',
  'midpoints',
  'centres',
  'intersections',
  'jobPoints',
];

/** The snap patch for the object snap toggle: all off when any is on, else all on. */
export function objectSnapToggle(snap: SnapOptions): Partial<SnapOptions> {
  const on = !OBJECT_SNAPS.some((key) => snap[key]);
  return Object.fromEntries(OBJECT_SNAPS.map((key) => [key, on]));
}

/** One entry of a toolbar drop-down. */
export interface PickerOption {
  id: number;
  name: string;
}

/** A toolbar drop-down: its entries, the chosen one, and whether it can change. */
export interface Picker {
  options: PickerOption[];
  value: number | null;
  disabled: boolean;
}

/**
 * The toolbar drop-down for items with an `active` flag (sheets, layers):
 * disabled while there is only one to choose from.
 */
export function activePicker(items: ReadonlyArray<PickerOption & { active: boolean }>): Picker {
  const active = items.find((item) => item.active);
  return {
    options: items.map(({ id, name }) => ({ id, name })),
    value: active?.id ?? null,
    disabled: items.length < 2,
  };
}

/**
 * The level drop-down, always shown (a job always has
 * a level); empty and disabled until the model's levels have been read.
 */
export function levelPicker(
  levels: ReadonlyArray<PickerOption>,
  activeLevel: number | null,
): Picker {
  return {
    // The model lists levels bottom first; the drop-down shows the top first.
    options: [...levels].reverse().map(({ id, name }) => ({ id, name })),
    value: activeLevel ?? levels[0]?.id ?? null,
    disabled: levels.length < 2,
  };
}

/**
 * How many toolbar groups, from the left, fit in `available` pixels when
 * the rest go behind a More button `moreWidth` wide. While a group's width
 * is not known yet every group shows (clipped), so all get measured.
 */
export function groupsThatFit(
  widths: ReadonlyArray<number | undefined>,
  available: number,
  moreWidth: number,
): number {
  const total = widths.reduce<number>((sum, w) => sum + (w ?? 0), 0);
  if (widths.some((w) => w === undefined) || total <= available) {
    return widths.length;
  }
  let used = 0;
  for (let i = 0; i < widths.length; i++) {
    used += widths[i] ?? 0;
    if (used > available - moreWidth) {
      return i;
    }
  }
  return widths.length;
}

/**
 * The tooltip of a view toolbar tool: its name, with the key that picks it
 * (Z for the zoom box; the pan tool has none, Space pans while held).
 */
export function viewToolTip(tool: 'zoomBox' | 'pan'): string {
  return tool === 'zoomBox' ? commandTip(strings.tools.zoomBox, 'tool.zoomBox') : strings.tools.pan;
}
