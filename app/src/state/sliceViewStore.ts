// Slice heights in the 3D view: while the Slice Model form is open, the view
// colours the slice picked in the Slice Heights list (red, with what is
// above it green) and turns a double click, Shift + double click and
// Shift + wheel into edits of that list, which the form makes.

import { create } from 'zustand';

/** The picked slice: `low` to `high` millimetres above the zero plane. */
export interface SliceBand {
  low: number;
  high: number;
}

/** An edit of the list asked for in the 3D view: a double click moves the
 * picked height to `z` (millimetres above the zero plane), Shift + double
 * click inserts one at `z`, Shift + wheel nudges the picked height by
 * `steps` notches (up positive). */
export type ViewHeightEdit =
  { kind: 'move'; z: number } | { kind: 'insert'; z: number } | { kind: 'nudge'; steps: number };

interface SliceViewState {
  /** True while the Slice Model form is open. */
  active: boolean;
  /** The picked slice, or null with New height picked. */
  band: SliceBand | null;
  /** The last edit asked for, with a count so the same edit twice is new. */
  request: (ViewHeightEdit & { seq: number }) | null;
  setActive: (active: boolean) => void;
  setBand: (band: SliceBand | null) => void;
  requestEdit: (edit: ViewHeightEdit) => void;
}

export const useSliceViewStore = create<SliceViewState>()((set, get) => ({
  active: false,
  band: null,
  request: null,
  setActive: (active) => set(active ? { active } : { active, band: null }),
  setBand: (band) => {
    const now = get().band;
    if (
      band === now ||
      (band !== null && now !== null && band.low === now.low && band.high === now.high)
    ) {
      return;
    }
    set({ band });
  },
  requestEdit: (edit) => set({ request: { ...edit, seq: (get().request?.seq ?? 0) + 1 } }),
}));

/** What a double click on the model asks for at height `z`: Shift inserts
 * a height, a plain one moves the picked height. */
export function doubleClickEdit(z: number, shift: boolean): ViewHeightEdit {
  return shift ? { kind: 'insert', z } : { kind: 'move', z };
}

/** The nudge a wheel turn asks for: only with Shift, one notch per event,
 * up (a negative delta) raising the height; null otherwise. Shift turns a
 * vertical wheel sideways on some systems, so either delta counts. */
export function wheelEdit(event: {
  deltaX: number;
  deltaY: number;
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}): ViewHeightEdit | null {
  if (!event.shiftKey || event.ctrlKey || event.metaKey || event.altKey) {
    return null;
  }
  const delta = event.deltaY !== 0 ? event.deltaY : event.deltaX;
  if (delta === 0) {
    return null;
  }
  return { kind: 'nudge', steps: delta < 0 ? 1 : -1 };
}
