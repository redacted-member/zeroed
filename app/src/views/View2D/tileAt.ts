// Double click on a Toolpath Tiling tile in the 2D view makes it the tile
// the views show.

import type { TileInfo } from '../../bridge/cam';

/** Index of the tile whose own area (without the overlap) holds the job
 * point, or null. */
export function tileAt(tiles: readonly TileInfo[], x: number, y: number): number | null {
  const index = tiles.findIndex(
    ({ nominal: [minX, minY, maxX, maxY] }) => x >= minX && x <= maxX && y >= minY && y <= maxY,
  );
  return index < 0 ? null : index;
}
