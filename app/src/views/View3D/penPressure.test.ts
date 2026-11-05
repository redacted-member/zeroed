import { describe, expect, it } from 'vitest';

import { penPressure } from './penPressure';

describe('pen pressure', () => {
  it('reads a pen only when tablet support is on', () => {
    expect(penPressure({ pointerType: 'pen', pressure: 0.3 }, true)).toBe(0.3);
    expect(penPressure({ pointerType: 'pen', pressure: 0.3 }, false)).toBeNull();
    expect(penPressure({ pointerType: 'mouse', pressure: 0.5 }, true)).toBeNull();
    expect(penPressure({ pointerType: 'pen', pressure: 1.4 }, true)).toBe(1);
  });
});
