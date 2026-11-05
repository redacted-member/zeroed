import { describe, expect, it } from 'vitest';

import {
  cutDepthText,
  depthValues,
  formatDuration,
  initialDepths,
  validateDepths,
} from './toolpathFields';

describe('toolpath depth fields', () => {
  it('reads depths in the job unit and uses the tool pass depth by default', () => {
    expect(depthValues(initialDepths('mm', 6), 'mm')).toEqual({
      startDepth: 0,
      cutDepth: 6,
      passDepth: null,
    });
    expect(depthValues(initialDepths('inch', 12.7), 'inch')).toEqual({
      startDepth: 0,
      cutDepth: 12.7,
      passDepth: null,
    });
  });

  it('measures the cut depth from the start depth', () => {
    const state = { ...initialDepths('mm', 6), startDepth: '6' };
    expect(validateDepths(state, 'mm').cutDepth).toBeUndefined();
    expect(depthValues(state, 'mm')?.cutDepth).toBe(12);
    expect(cutDepthText(6, 12, 'mm')).toBe('6');
    expect(validateDepths({ ...state, cutDepth: '0' }, 'mm').cutDepth).toBeDefined();
  });

  it('formats machining time as hours, minutes and seconds', () => {
    expect(formatDuration(0)).toBe('0:00:00');
    expect(formatDuration(3725.4)).toBe('1:02:05');
  });
});
