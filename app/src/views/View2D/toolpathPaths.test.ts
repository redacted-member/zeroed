import { describe, expect, it } from 'vitest';

import { MOVE_KIND, type MovesReply } from '../../bridge/cam';
import { ALL_MOVES } from '../../lib/moveGroups';
import { straightRuns, toolpathPaths } from './toolpathPaths';

const moves: MovesReply = {
  revision: 1,
  toolpaths: [{ id: 1, offset: 0, count: 6, visible: true }],
  // prettier-ignore
  xyz: new Float32Array([
    0, 0, 10,
    5, 5, 10,
    5, 5, -2,
    15, 5, -2,
    20, 8, -2,
    20, 8, 10,
  ]),
  width: new Float32Array([0, 0, 6, 6, 3, 0]),
  kind: new Uint8Array([
    MOVE_KIND.rapid,
    MOVE_KIND.rapid,
    MOVE_KIND.plunge,
    MOVE_KIND.feed,
    MOVE_KIND.join,
    MOVE_KIND.retract,
  ]),
};

describe('2D toolpath paths', () => {
  it('draws sideways moves by group and drops vertical ones', () => {
    const paths = toolpathPaths(moves, ALL_MOVES, false);
    expect([...paths.lines.rapid]).toEqual([0, 0, 5, 5]);
    expect([...paths.lines.cut]).toEqual([5, 5, 15, 5]);
    expect([...paths.lines.join]).toEqual([15, 5, 20, 8]);
    expect(paths.lines.plunge.length).toBe(0);
    expect(paths.lines.retract.length).toBe(0);
    expect(paths.solid).toEqual([]);
  });

  it('hides the groups turned off', () => {
    const paths = toolpathPaths(moves, { ...ALL_MOVES, rapid: false, join: false }, false);
    expect(paths.lines.rapid.length).toBe(0);
    expect(paths.lines.join.length).toBe(0);
    expect(paths.lines.cut.length).toBe(4);
  });

  it('buckets cutting moves by the width they cut for a solid preview', () => {
    const paths = toolpathPaths(moves, ALL_MOVES, true);
    expect(paths.solid.map((s) => s.width)).toEqual([6]);
    expect(paths.solid[0]?.segments.length).toBe(8);
  });
});

describe('straight runs', () => {
  it('joins segments that carry straight on and keeps turns, gaps and reversals apart', () => {
    const segments = Float32Array.from([
      // A raster line of three moves.
      0, 0, 1, 0, 1, 0, 2, 0, 2, 0, 3, 0,
      // A step over (a turn), then back the other way (a reversal of the step).
      3, 0, 3, 1, 3, 1, 3, 0.5,
      // A gap: this one starts somewhere else.
      5, 5, 6, 6, 6, 6, 7, 7,
    ]);
    expect(straightRuns(segments)).toEqual([
      [0, 0, 3, 0],
      [3, 0, 3, 1],
      [3, 1, 3, 0.5],
      [5, 5, 7, 7],
    ]);
    expect(straightRuns(new Float32Array(0))).toEqual([]);
  });
});
