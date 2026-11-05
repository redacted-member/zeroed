import type { MouseEvent } from 'react';

/** Where a right click menu opens: at the pointer, or under the element when
 * the menu key or Shift+F10 opened it (those report no pointer position). */
export function menuAnchor(event: MouseEvent<HTMLElement>): { x: number; y: number } {
  if (event.clientX !== 0 || event.clientY !== 0) {
    return { x: event.clientX, y: event.clientY };
  }
  const rect = event.currentTarget.getBoundingClientRect();
  return { x: rect.left + 8, y: rect.bottom };
}
