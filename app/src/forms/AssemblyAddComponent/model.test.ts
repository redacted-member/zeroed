import { describe, expect, it } from 'vitest';

import { filterComponents, staleThumbnails } from './model';

const entries = [
  { id: 1, label: 'Screen (Front)' },
  { id: 2, label: 'Knob' },
  { id: 3, label: 'Front panel' },
];

describe('the Add Component list', () => {
  it('keeps every component for an empty search', () => {
    expect(filterComponents(entries, '').map((e) => e.id)).toEqual([1, 2, 3]);
    expect(filterComponents(entries, '   ').map((e) => e.id)).toEqual([1, 2, 3]);
  });

  it('keeps the ones whose name holds every word, in any case or order', () => {
    expect(filterComponents(entries, 'front').map((e) => e.id)).toEqual([1, 3]);
    expect(filterComponents(entries, 'PANEL front').map((e) => e.id)).toEqual([3]);
    expect(filterComponents(entries, 'drawer')).toEqual([]);
  });

  it('lists the components picked in the Component Tree first', () => {
    expect(filterComponents(entries, '', [3]).map((e) => e.id)).toEqual([3, 1, 2]);
    expect(filterComponents(entries, 'front', [2, 3]).map((e) => e.id)).toEqual([3, 1]);
  });

  it('loads the thumbnails not loaded yet, and again when a shape changes', () => {
    const wanted = [
      { id: 1, look: 'a' },
      { id: 2, look: 'b' },
      { id: 3, look: 'c' },
    ];
    expect(staleThumbnails(wanted, new Map([[2, 'b']]))).toEqual([1, 3]);
    expect(
      staleThumbnails(
        wanted,
        new Map([
          [1, 'a'],
          [2, 'old'],
          [3, 'c'],
        ]),
      ),
    ).toEqual([2]);
    expect(staleThumbnails([{ id: 4 }], new Map([[4, '']]))).toEqual([]);
  });
});
