import { Matrix4 } from 'three';
import { describe, expect, it } from 'vitest';

import { MOVE_KIND, type MovesReply } from '../../bridge/cam';
import { blankTurnMatrix, movedBox, restingTurn } from './blankTurn';

/** Cutting moves at these distances round a blank, along X. */
function cuts(arounds: number[], visible = true): MovesReply {
  const xyz: number[] = [];
  for (const around of arounds) {
    xyz.push(10, around, 18);
  }
  return {
    revision: 1,
    toolpaths: [{ id: 1, offset: 0, count: arounds.length, visible }],
    xyz: new Float32Array(xyz),
    width: new Float32Array(arounds.length),
    kind: new Uint8Array(arounds.map(() => MOVE_KIND.feed)),
  };
}

describe('restingTurn', () => {
  const radius = 20;
  const quarter = (Math.PI * 40) / 4;

  it('brings a design up to face the viewer', () => {
    expect(restingTurn(cuts([quarter]), 'x', radius)).toBeCloseTo(Math.PI / 2, 5);
    expect(restingTurn(cuts([quarter - 2, quarter + 2]), 'x', radius)).toBeCloseTo(Math.PI / 2, 5);
    expect(restingTurn(cuts([2 * quarter]), 'x', radius)).toBeCloseTo(Math.PI, 5);
  });

  it('takes the mean the short way across the seam', () => {
    const round = 4 * quarter;
    expect(restingTurn(cuts([3, round - 3]), 'x', radius)).toBeCloseTo(0, 5);
    expect(Math.abs(restingTurn(cuts([round / 2 - 3, round / 2 + 3]), 'x', radius))).toBeCloseTo(
      Math.PI,
      5,
    );
  });

  it('leaves a job cut all the way round where it is', () => {
    expect(restingTurn(cuts([0, quarter, 2 * quarter, 3 * quarter]), 'x', radius)).toBe(0);
  });

  it('leaves the blank alone with nothing to show', () => {
    expect(restingTurn(cuts([]), 'x', radius)).toBe(0);
    expect(restingTurn(cuts([quarter], false), 'x', radius)).toBe(0);
    expect(restingTurn(cuts([quarter]), 'x', 0)).toBe(0);
  });

  it('ignores rapids', () => {
    const moves = cuts([quarter, 2 * quarter]);
    moves.kind[1] = MOVE_KIND.rapid;
    expect(restingTurn(moves, 'x', radius)).toBeCloseTo(Math.PI / 2, 5);
  });

  it('reads flat X round a blank along Y', () => {
    const moves: MovesReply = {
      revision: 1,
      toolpaths: [{ id: 1, offset: 0, count: 1, visible: true }],
      xyz: new Float32Array([quarter, 99, 18]),
      width: new Float32Array(1),
      kind: new Uint8Array([MOVE_KIND.feed]),
    };
    expect(restingTurn(moves, 'y', radius)).toBeCloseTo(Math.PI / 2, 5);
  });
});

describe('movedBox', () => {
  const box = { min: [1, -2, -30], max: [3, 0, -28] } as const;

  it('leaves a box where it is with no turn', () => {
    const same = movedBox(box, new Matrix4());
    expect(same.min).toEqual([1, -2, -30]);
    expect(same.max).toEqual([3, 0, -28]);
  });

  it('frames the spot where the turned blank carried it', () => {
    // On a blank of radius 20 a spot a quarter of the way round, at scene z
    // -20 and level with the axis at y -20, comes up to the top.
    const spot = { min: [5, -20, -20], max: [5, -20, -20] } as const;
    const moved = movedBox(spot, blankTurnMatrix('x', 20, Math.PI / 2));
    expect(moved.min[0]).toBeCloseTo(5, 9);
    expect(moved.min[1]).toBeCloseTo(0, 9);
    expect(moved.min[2]).toBeCloseTo(0, 9);
    expect(moved.max[1]).toBeCloseTo(0, 9);
  });
});
