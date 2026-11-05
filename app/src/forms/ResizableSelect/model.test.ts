import { describe, expect, it } from 'vitest';

import {
  MAX_LIST_SIZE,
  MIN_LIST_HEIGHT,
  MIN_LIST_WIDTH,
  PAGE_ROWS,
  clampListSize,
  draggedSize,
  keyedIndex,
  typeAhead,
} from './model';

describe('resizable drop-down list model', () => {
  it('keeps saved sizes in range and treats zero as the usual size', () => {
    expect(clampListSize({ width: 0, height: 0 })).toBeNull();
    expect(clampListSize(null)).toBeNull();
    expect(clampListSize({ width: 10, height: 9000 })).toEqual({
      width: MIN_LIST_WIDTH,
      height: MAX_LIST_SIZE,
    });
    expect(clampListSize({ width: 300.4, height: 500.6 })).toEqual({ width: 300, height: 501 });
  });

  it('grows with the corner drag and never below the smallest size', () => {
    const start = { width: 200, height: 264 };
    expect(draggedSize(start, 50, 100)).toEqual({ width: 250, height: 364 });
    expect(draggedSize(start, -500, -500)).toEqual({
      width: MIN_LIST_WIDTH,
      height: MIN_LIST_HEIGHT,
    });
  });

  it('moves the highlight with the arrow, page and end keys', () => {
    expect(keyedIndex(3, 50, 'ArrowDown')).toBe(4);
    expect(keyedIndex(0, 50, 'ArrowUp')).toBe(0);
    expect(keyedIndex(-1, 50, 'ArrowDown')).toBe(0);
    expect(keyedIndex(45, 50, 'PageDown')).toBe(49);
    expect(keyedIndex(20, 50, 'PageUp')).toBe(20 - PAGE_ROWS);
    expect(keyedIndex(20, 50, 'End')).toBe(49);
    expect(keyedIndex(20, 50, 'Home')).toBe(0);
    expect(keyedIndex(20, 50, 'x')).toBeNull();
    expect(keyedIndex(0, 0, 'ArrowDown')).toBeNull();
  });

  it('finds options by their first letters and steps through repeats', () => {
    const labels = ['Arial', 'Courier', 'Comic', 'Georgia', 'Times'];
    expect(typeAhead(labels, 'c', -1)).toBe(1);
    expect(typeAhead(labels, 'c', 1)).toBe(2);
    expect(typeAhead(labels, 'c', 2)).toBe(1);
    expect(typeAhead(labels, 'com', 1)).toBe(2);
    expect(typeAhead(labels, 'z', 0)).toBe(-1);
  });
});
