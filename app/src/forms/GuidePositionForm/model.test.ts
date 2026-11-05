import { describe, expect, it } from 'vitest';

import { guidePositionError, guidePositionMm, guideStateFrom } from './model';

describe('guide position model', () => {
  const guide = { id: 1, axis: 'x' as const, position: 25.4, locked: false };

  it('shows the position in the display unit', () => {
    expect(guideStateFrom(guide, 'mm').position).toBe('25.4');
    expect(guideStateFrom(guide, 'inch').position).toBe('1');
  });

  it('converts a typed inch value back to millimetres', () => {
    expect(guidePositionMm({ position: '2', locked: false }, 'inch')).toBe(50.8);
    expect(guidePositionMm({ position: 'abc', locked: false }, 'mm')).toBeNull();
    expect(guidePositionError({ position: '', locked: false }, 'mm')).toBeDefined();
  });
});
