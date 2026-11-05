// Pointer cursors for the three interaction modes of the drawing view:
// selection (arrow, move over a selected object, resize arrows on the scale
// handles), node editing (arrow with a node square) and transform (rotate
// cursor on the rotation handles).

import type { HandleId } from '../renderer';

function svgCursor(body: string, hotX: number, hotY: number, fallback: string): string {
  const svg =
    `<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24'>` +
    `<g fill='none' stroke-linejoin='round' stroke-linecap='round'>${body}</g></svg>`;
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}") ${hotX} ${hotY}, ${fallback}`;
}

const ARROW = 'M2 2 L2 17 L6 13 L9 20 L12 19 L9 12 L14 12 Z';

/** Selection mode, nothing under the pointer. */
export const SELECT_CURSOR = 'default';

/** Over a selected object: dragging moves it. */
export const MOVE_CURSOR = 'move';

/** Node editing mode: an arrow with a small node square. */
export const NODE_CURSOR = svgCursor(
  `<path d='${ARROW}' fill='black' stroke='white' stroke-width='1.2'/>` +
    `<rect x='15' y='15' width='6' height='6' fill='white' stroke='black' stroke-width='1.4'/>`,
  2,
  2,
  'crosshair',
);

/** Transform mode, over a rotation handle. */
export const ROTATE_CURSOR = svgCursor(
  `<path d='M18 8 A7 7 0 1 0 19 14' stroke='white' stroke-width='4'/>` +
    `<path d='M18 8 A7 7 0 1 0 19 14' stroke='black' stroke-width='2'/>` +
    `<path d='M14 7 L19 8 L19 3' stroke='white' stroke-width='4'/>` +
    `<path d='M14 7 L19 8 L19 3' stroke='black' stroke-width='2'/>`,
  12,
  12,
  'alias',
);

/** Cursor over a selection handle. */
export function handleCursor(handle: HandleId): string {
  switch (handle) {
    case 'rotate':
      return ROTATE_CURSOR;
    case 'pivot':
      return MOVE_CURSOR;
    case 'nw':
    case 'se':
      return 'nwse-resize';
    case 'ne':
    case 'sw':
      return 'nesw-resize';
    case 'n':
    case 's':
      return 'ns-resize';
    case 'e':
    case 'w':
      return 'ew-resize';
  }
}
