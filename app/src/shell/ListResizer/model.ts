// Height maths for the divider above a dock list (the toolpath list, the
// Component Tree): dragging it up gives the list more rows.

import { MIN_LIST_HEIGHT, clampListHeight } from '../../lib/prefs';

/** Pixels an arrow key moves the divider; Shift moves it four times as far. */
export const LIST_STEP_PX = 16;

/** The list height after the divider moved `dy` pixels (down is positive). */
export function draggedHeight(startHeight: number, dy: number): number {
  return clampListHeight(Math.max(MIN_LIST_HEIGHT, startHeight - dy));
}

/** The list height after `key` on the focused divider, or null when the key does nothing. */
export function keyedHeight(height: number, key: string, shift: boolean): number | null {
  const step = shift ? LIST_STEP_PX * 4 : LIST_STEP_PX;
  switch (key) {
    case 'ArrowUp':
      return draggedHeight(height, -step);
    case 'ArrowDown':
      return draggedHeight(height, step);
    case 'Home':
      return MIN_LIST_HEIGHT;
    case 'Escape':
      // Back to sharing the space with the tools above.
      return 0;
    default:
      return null;
  }
}
