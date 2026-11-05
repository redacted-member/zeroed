import { describe, expect, it } from 'vitest';

import { anyHidden, isolateNext, showAllWanted } from './model';

describe('piece tree', () => {
  it('offers Show All while a piece is hidden or one is isolated', () => {
    expect(anyHidden([{ hidden: false }, {}])).toBe(false);
    expect(anyHidden([{}, { hidden: true }])).toBe(true);
    expect(showAllWanted([{}, {}], null)).toBe(false);
    expect(showAllWanted([{}, {}], '1:1')).toBe(true);
    expect(showAllWanted([{}, { hidden: true }], null)).toBe(true);
  });

  it('isolates a piece, and Isolate on the isolated piece ends Isolate', () => {
    expect(isolateNext(null, '1:1')).toBe('1:1');
    expect(isolateNext('2:1', '1:1')).toBe('1:1');
    expect(isolateNext('1:1', '1:1')).toBeNull();
  });
});
