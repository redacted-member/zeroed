import { describe, expect, it } from 'vitest';

import type { TileInfo } from '../../bridge/cam';
import { tileAt } from './tileAt';

const tile = (minX: number, maxX: number): TileInfo => ({
  row: 0,
  column: 0,
  nominal: [minX, 0, maxX, 100],
  area: [minX - 10, 0, maxX, 100],
  shift: [0, 0],
});

describe('tile under the pointer', () => {
  it('finds the tile by its own area, not the overlap', () => {
    const tiles = [tile(0, 120), tile(120, 240), tile(240, 300)];
    expect(tileAt(tiles, 170, 50)).toBe(1);
    expect(tileAt(tiles, 235, 50)).toBe(1);
    expect(tileAt(tiles, 250, 50)).toBe(2);
    expect(tileAt(tiles, 400, 50)).toBeNull();
    expect(tileAt([], 1, 1)).toBeNull();
  });
});
