// Pure state for the Create Cross Section Vector tool: two clicks across
// the model in the 2D view. The first sets the start of the section line,
// the second its end, and the vector is made at once.

import type { Path, Point } from '../../bridge/document';
import type { CrossSectionArgs } from '../../bridge/modelTools';
import { polylinePath } from '../../lib/geometry';

export interface CrossSectionState {
  /** The first click, or null while the next click starts a line. */
  start: Point | null;
}

export const INITIAL_CROSS_SECTION: CrossSectionState = { start: null };

/** What a click at `p` (job millimetres) does: the first one keeps the
 * start, the second answers the section to make and starts over. A second
 * click on the first makes nothing. */
export function clickPoint(
  state: CrossSectionState,
  p: Point,
): { state: CrossSectionState; section: CrossSectionArgs | null } {
  if (state.start === null) {
    return { state: { start: { x: p.x, y: p.y } }, section: null };
  }
  const from = state.start;
  if (Math.hypot(p.x - from.x, p.y - from.y) < 1e-6) {
    return { state, section: null };
  }
  return { state: INITIAL_CROSS_SECTION, section: { from, to: { x: p.x, y: p.y } } };
}

/** The section line as a preview path, empty until the first end is set. */
export function sectionPreview(from: Point | null, to: Point | null): Path[] {
  if (from === null || to === null) {
    return [];
  }
  return [polylinePath([from, to])];
}
