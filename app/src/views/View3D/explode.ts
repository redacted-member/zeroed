// The exploded view: each piece moved by a share of its full explode
// offset, or, when the job has named explode steps, by the steps played in
// order. A view state only; the pieces' stored places do not change.

import { pieceKey, type ExplodeStep, type PieceRow, type Triple } from '../../bridge/assembly';

/** The pieces moved `fraction` (0 to 1) of the way out along their offsets. */
export function explodedPieces(
  pieces: readonly PieceRow[],
  offsets: Readonly<Record<string, Triple>>,
  fraction: number,
): PieceRow[] {
  if (fraction <= 0) {
    return [...pieces];
  }
  return pieces.map((piece) => {
    const offset = offsets[pieceKey(piece)];
    if (offset === undefined) {
      return piece;
    }
    return {
      ...piece,
      position: [
        piece.position[0] + offset[0] * fraction,
        piece.position[1] + offset[1] * fraction,
        piece.position[2] + offset[2] * fraction,
      ],
    };
  });
}

/**
 * How far each piece is moved at `fraction` (0 to 1) of the explode: with
 * no steps, that share of its offset; with steps, the steps played in
 * order, each over an equal share of the slider (the first step's pieces
 * move out, then the second's), a piece in several steps adding them up.
 */
export function explodeMoves(
  offsets: Readonly<Record<string, Triple>>,
  fraction: number,
  steps: readonly ExplodeStep[],
): Record<string, Triple> {
  const out: Record<string, Triple> = {};
  if (fraction <= 0) {
    return out;
  }
  const add = (key: string, by: Triple, share: number) => {
    const now = out[key] ?? [0, 0, 0];
    out[key] = [now[0] + by[0] * share, now[1] + by[1] * share, now[2] + by[2] * share];
  };
  if (steps.length === 0) {
    for (const [key, offset] of Object.entries(offsets)) {
      add(key, offset, fraction);
    }
    return out;
  }
  steps.forEach((step, i) => {
    const share = Math.min(1, Math.max(0, fraction * steps.length - i));
    if (share <= 0) {
      return;
    }
    const by: Triple = [
      step.direction[0] * step.distance,
      step.direction[1] * step.distance,
      step.direction[2] * step.distance,
    ];
    for (const piece of step.pieces) {
      add(pieceKey(piece), by, share);
    }
  });
  return out;
}
