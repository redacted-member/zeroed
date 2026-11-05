import { describe, expect, it } from 'vitest';

import { MAX_DOCK_WIDTH, MIN_DOCK_WIDTH } from '../../lib/prefs';
import { KEY_STEP_PX, draggedWidth, keyedWidth } from './model';

describe('dock resizer', () => {
  it('grows a left dock as its edge moves right and a right dock as it moves left', () => {
    expect(draggedWidth('left', 268, 40)).toBe(308);
    expect(draggedWidth('right', 300, 40)).toBe(260);
    expect(draggedWidth('right', 300, -52.4)).toBe(352);
  });

  it('keeps the width inside the dock range', () => {
    expect(draggedWidth('left', 268, -500)).toBe(MIN_DOCK_WIDTH);
    expect(draggedWidth('right', 300, -900)).toBe(MAX_DOCK_WIDTH);
  });

  it('moves the edge with the arrow keys, Shift for bigger steps, Home and End for the limits', () => {
    expect(keyedWidth('left', 268, 'ArrowRight', false)).toBe(268 + KEY_STEP_PX);
    expect(keyedWidth('right', 300, 'ArrowLeft', false)).toBe(300 + KEY_STEP_PX);
    expect(keyedWidth('right', 300, 'ArrowRight', true)).toBe(300 - 4 * KEY_STEP_PX);
    expect(keyedWidth('left', 300, 'Home', false)).toBe(MIN_DOCK_WIDTH);
    expect(keyedWidth('left', 300, 'End', false)).toBe(MAX_DOCK_WIDTH);
    expect(keyedWidth('left', 300, 'a', false)).toBeNull();
  });
});
