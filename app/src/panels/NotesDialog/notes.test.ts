import { describe, expect, it } from 'vitest';

import { showsOnOpen } from './notes';

describe('showsOnOpen', () => {
  it('shows notes that start with a full stop', () => {
    expect(showsOnOpen('.Clamp at the corners')).toBe(true);
    expect(showsOnOpen('  .indented')).toBe(true);
    expect(showsOnOpen('Customer: Smith')).toBe(false);
    expect(showsOnOpen('')).toBe(false);
  });
});
