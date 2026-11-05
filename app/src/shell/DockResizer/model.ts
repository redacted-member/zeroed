// Width maths for dragging or keying a dock panel's inner edge.

import { MAX_DOCK_WIDTH, MIN_DOCK_WIDTH, clampDockWidth } from '../../lib/prefs';

/** Which side of the window the dock sits on. */
export type DockSide = 'left' | 'right';

/** Pixels an arrow key moves the edge; Shift moves it four times as far. */
export const KEY_STEP_PX = 16;

/**
 * The width after the edge moved `dx` pixels from where a drag started. A
 * left dock grows as its right edge moves right; a right dock grows as its
 * left edge moves left.
 */
export function draggedWidth(side: DockSide, startWidth: number, dx: number): number {
  return clampDockWidth(side === 'left' ? startWidth + dx : startWidth - dx);
}

/** The width after `key` on the focused edge, or null when the key does nothing. */
export function keyedWidth(
  side: DockSide,
  width: number,
  key: string,
  shift: boolean,
): number | null {
  const step = shift ? KEY_STEP_PX * 4 : KEY_STEP_PX;
  switch (key) {
    case 'ArrowLeft':
      return draggedWidth(side, width, -step);
    case 'ArrowRight':
      return draggedWidth(side, width, step);
    case 'Home':
      return MIN_DOCK_WIDTH;
    case 'End':
      return MAX_DOCK_WIDTH;
    default:
      return null;
  }
}
