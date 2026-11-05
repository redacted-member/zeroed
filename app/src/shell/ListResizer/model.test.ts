import { describe, expect, it } from 'vitest';

import { MIN_LIST_HEIGHT } from '../../lib/prefs';
import { LIST_STEP_PX, draggedHeight, keyedHeight } from './model';

describe('dock list divider', () => {
  it('grows the list as the divider moves up and keeps it above the minimum', () => {
    expect(draggedHeight(200, -50)).toBe(250);
    expect(draggedHeight(200, 30.4)).toBe(170);
    expect(draggedHeight(200, 500)).toBe(MIN_LIST_HEIGHT);
  });

  it('moves with the arrow keys and goes back to sharing the space with Escape', () => {
    expect(keyedHeight(200, 'ArrowUp', false)).toBe(200 + LIST_STEP_PX);
    expect(keyedHeight(200, 'ArrowDown', true)).toBe(200 - 4 * LIST_STEP_PX);
    expect(keyedHeight(200, 'Home', false)).toBe(MIN_LIST_HEIGHT);
    expect(keyedHeight(200, 'Escape', false)).toBe(0);
    expect(keyedHeight(200, 'x', false)).toBeNull();
  });
});
