import { describe, expect, it } from 'vitest';

import { DEFAULT_BRUSH, strokeBrush } from './sculptStore';

describe('sculpt strokes', () => {
  it('erases with Shift and the Undo Brush only', () => {
    const undo = { ...DEFAULT_BRUSH, brush: 'restore' as const };
    expect(strokeBrush(undo, true).brush).toBe('erase');
    expect(strokeBrush(undo, false).brush).toBe('restore');
    expect(strokeBrush(DEFAULT_BRUSH, true).brush).toBe('deposit');
  });
});
