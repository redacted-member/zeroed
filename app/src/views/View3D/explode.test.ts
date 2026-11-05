import { describe, expect, it } from 'vitest';

import type { PieceRow } from '../../bridge/assembly';
import { explodedPieces, explodeMoves } from './explode';

const piece = (sheet: number): PieceRow => ({
  sheet,
  part: 1,
  parts: 1,
  sheetName: 'S',
  vectorName: null,
  model: false,
  position: [10, 20, 30],
  rotation: [0, 0, 0],
  size: [1, 1, 1],
  hasModel: false,
  hasToolpaths: true,
});

describe('explodedPieces', () => {
  it('moves each piece its share of its offset and leaves the rest', () => {
    const shown = explodedPieces([piece(1), piece(2)], { '2:1': [0, 0, 40] }, 0.25);
    expect(shown[0]?.position).toEqual([10, 20, 30]);
    expect(shown[1]?.position).toEqual([10, 20, 40]);
    expect(explodedPieces([piece(2)], { '2:1': [0, 0, 40] }, 0)[0]?.position).toEqual([10, 20, 30]);
  });
});

describe('explodeMoves', () => {
  it('moves each piece its share of its offset when there are no steps', () => {
    expect(explodeMoves({ '1:1': [0, 0, 40] }, 0.5, [])).toEqual({ '1:1': [0, 0, 20] });
    expect(explodeMoves({ '1:1': [0, 0, 40] }, 0, [])).toEqual({});
  });

  it('plays named steps in order, each over its share of the slider', () => {
    const steps = [
      {
        id: 1,
        name: 'Top off',
        pieces: [{ sheet: 2, part: 1 }],
        direction: [0, 0, 1],
        distance: 100,
      },
      {
        id: 2,
        name: 'Side out',
        pieces: [
          { sheet: 2, part: 1 },
          { sheet: 3, part: 1 },
        ],
        direction: [1, 0, 0],
        distance: 50,
      },
    ] as const;
    const list = steps.map((s) => ({
      ...s,
      pieces: [...s.pieces],
      direction: [...s.direction] as [number, number, number],
    }));
    expect(explodeMoves({ '2:1': [9, 9, 9] }, 0.25, list)).toEqual({ '2:1': [0, 0, 50] });
    expect(explodeMoves({}, 0.75, list)).toEqual({ '2:1': [25, 0, 100], '3:1': [25, 0, 0] });
    expect(explodeMoves({}, 1, list)).toEqual({ '2:1': [50, 0, 100], '3:1': [50, 0, 0] });
  });
});
