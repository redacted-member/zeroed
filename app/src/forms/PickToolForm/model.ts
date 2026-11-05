// Pure helpers for Interactive Vector Trim: when a drag has moved far
// enough to trim again, and which trimmed vectors are left to rejoin.

import type { Point } from '../../bridge/document';

/** While the button is held, the pointer trims again once it has moved
 * this many pick tolerances from the last place it trimmed. */
const STEP = 1;

/** Whether a drag at `at` should trim the span there: the first press
 * always does, later moves once they are clear of the last trim. */
export function trimsHere(last: Point | null, at: Point, tolerance: number): boolean {
  return last === null || Math.hypot(at.x - last.x, at.y - last.y) >= tolerance * STEP;
}

/** The stretch of a drag to cut next: from where the last cut ended to
 * the pointer, once the pointer is clear of that place; null until then.
 * Cutting along the stretch, not only at the pointer, trims every span the
 * pointer passed over however fast it moved. */
export function bladeTo(
  from: Point | null,
  at: Point,
  tolerance: number,
): { from: Point; to: Point } | null {
  if (from === null || !trimsHere(from, at, tolerance)) {
    return null;
  }
  return { from, to: at };
}

/** The trimmed vectors that still exist, in the order they were trimmed,
 * which Rejoin Trimmed Sections joins when the form closes. */
export function rejoinIds(trimmed: readonly number[], existing: ReadonlySet<number>): number[] {
  const out: number[] = [];
  for (const id of trimmed) {
    if (existing.has(id) && !out.includes(id)) {
      out.push(id);
    }
  }
  return out;
}

/** How close two ends must be for the rejoin to join them, in millimetres. */
export const REJOIN_TOLERANCE = 0.01;
