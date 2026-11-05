// The piece tree's model: which pieces show and what Isolate does next.

import type { PieceRow } from '../../bridge/assembly';

/** Whether any piece is saved hidden. */
export function anyHidden(pieces: readonly Pick<PieceRow, 'hidden'>[]): boolean {
  return pieces.some((p) => p.hidden === true);
}

/** Whether Show All has something to do: a piece is hidden or one is isolated. */
export function showAllWanted(
  pieces: readonly Pick<PieceRow, 'hidden'>[],
  isolated: string | null,
): boolean {
  return isolated !== null || anyHidden(pieces);
}

/** The piece isolated after Isolate on `key`: again on the isolated piece ends Isolate. */
export function isolateNext(isolated: string | null, key: string): string | null {
  return isolated === key ? null : key;
}
