// Sizes, keys and type-ahead for a drop-down list whose open list can be
// dragged bigger by its corner, such as the font list.

/** Smallest and largest an open list can be dragged, in CSS pixels. */
export const MIN_LIST_WIDTH = 120;
export const MIN_LIST_HEIGHT = 80;
export const MAX_LIST_SIZE = 4000;
/** Rows an open list shows before it scrolls, at its usual size. */
export const USUAL_ROWS = 12;
/** Rows Page Up and Page Down move. */
export const PAGE_ROWS = 10;

export interface ListSize {
  width: number;
  height: number;
}

/** A saved size kept in range, or null (the usual size) when unset. */
export function clampListSize(size: ListSize | null | undefined): ListSize | null {
  if (
    size === null ||
    size === undefined ||
    !Number.isFinite(size.width) ||
    !Number.isFinite(size.height) ||
    size.width <= 0 ||
    size.height <= 0
  ) {
    return null;
  }
  return {
    width: Math.min(MAX_LIST_SIZE, Math.max(MIN_LIST_WIDTH, Math.round(size.width))),
    height: Math.min(MAX_LIST_SIZE, Math.max(MIN_LIST_HEIGHT, Math.round(size.height))),
  };
}

/** The list size after its corner moved `dx`, `dy` pixels from `start`. */
export function draggedSize(start: ListSize, dx: number, dy: number): ListSize {
  return (
    clampListSize({
      width: Math.max(MIN_LIST_WIDTH, start.width + dx),
      height: Math.max(MIN_LIST_HEIGHT, start.height + dy),
    }) ?? start
  );
}

/** The option `key` moves to from `index` among `count`, or null for other keys. */
export function keyedIndex(index: number, count: number, key: string): number | null {
  if (count === 0) {
    return null;
  }
  const clamp = (i: number) => Math.min(count - 1, Math.max(0, i));
  switch (key) {
    case 'ArrowDown':
      return clamp(index < 0 ? 0 : index + 1);
    case 'ArrowUp':
      return clamp(index < 0 ? 0 : index - 1);
    case 'Home':
      return 0;
    case 'End':
      return count - 1;
    case 'PageDown':
      return clamp(index + PAGE_ROWS);
    case 'PageUp':
      return clamp(index - PAGE_ROWS);
    default:
      return null;
  }
}

/**
 * The first option after `from` (wrapping round) whose label starts with
 * `query`, ignoring case; -1 when none does. A query of one letter typed
 * again steps to the next match.
 */
export function typeAhead(labels: readonly string[], query: string, from: number): number {
  const q = query.toLocaleLowerCase();
  if (q.length === 0 || labels.length === 0) {
    return -1;
  }
  const start = q.length === 1 ? from + 1 : Math.max(0, from);
  for (let k = 0; k < labels.length; k++) {
    const i = (((start + k) % labels.length) + labels.length) % labels.length;
    if (labels[i].toLocaleLowerCase().startsWith(q)) {
      return i;
    }
  }
  return -1;
}
