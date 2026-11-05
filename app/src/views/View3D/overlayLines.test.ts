import { describe, expect, it } from 'vitest';

import { MOVE_KIND, type MovesReply } from '../../bridge/cam';
import { DEFAULT_JOB, type JobSetup } from '../../bridge/types';
import { toolpathLines, vectorLines, wrappedToolpathLines } from './overlayLines';

describe('toolpath lines', () => {
  const moves: MovesReply = {
    revision: 3,
    toolpaths: [
      { id: 1, offset: 0, count: 4, visible: true },
      { id: 2, offset: 4, count: 2, visible: false },
    ],
    // prettier-ignore
    xyz: new Float32Array([
      0, 0, 5,
      10, 20, 5,
      10, 20, -2,
      30, 20, -2,
      0, 0, 5,
      9, 9, 9,
    ]),
    width: new Float32Array(6),
    kind: new Uint8Array([
      MOVE_KIND.rapid,
      MOVE_KIND.rapid,
      MOVE_KIND.plunge,
      MOVE_KIND.feed,
      MOVE_KIND.rapid,
      MOVE_KIND.feed,
    ]),
  };

  it('splits rapids, plunges and cuts, maps to scene axes and skips hidden toolpaths', () => {
    const lines = toolpathLines(moves, 0);
    expect([...lines.rapid]).toEqual([0, 5, -0, 10, 5, -20]);
    expect([...lines.plunge]).toEqual([10, 5, -20, 10, -2, -20]);
    expect([...lines.cut]).toEqual([10, -2, -20, 30, -2, -20]);
  });

  it('leaves out the groups the drawing toggles hide', () => {
    const lines = toolpathLines(moves, 0, {
      cut: true,
      rapid: false,
      plunge: false,
      retract: true,
      join: true,
    });
    expect(lines.rapid.length).toBe(0);
    expect(lines.plunge.length).toBe(0);
    expect(lines.cut.length).toBe(6);
  });

  it('puts Z zero on the material bottom at the thickness below the top', () => {
    const lines = toolpathLines(moves, 18);
    expect(lines.rapid[1]).toBe(-13);
  });
});

describe('vector lines', () => {
  it('draws open runs point to point and closes closed ones', () => {
    const open = { runs: [new Float32Array([0, 0, 10, 0, 10, 5])], closed: false };
    const closed = { runs: [new Float32Array([0, 0, 1, 0, 1, 1])], closed: true };
    expect(vectorLines([open], 0.1).length).toBe(2 * 6);
    const lines = vectorLines([closed], 0.1);
    expect(lines.length).toBe(3 * 6);
    // The closing segment runs from the last point back to the first.
    expect([...lines.slice(12)].map((v) => Math.round(v * 10) / 10)).toEqual([
      1, 0.1, -1, 0, 0.1, -0,
    ]);
  });
});

describe('wrapped toolpath lines', () => {
  const job = (axis: 'x' | 'y'): JobSetup => ({
    ...DEFAULT_JOB,
    kind: 'rotary',
    rotary: { diameter: 40, axis, length: 200, blank: 'cylinder', zZero: 'centre' },
  });
  const quarter = (Math.PI * 40) / 4;
  const reply = (xyz: number[], kind: number[]): MovesReply => ({
    revision: 1,
    toolpaths: [{ id: 1, offset: 0, count: kind.length, visible: true }],
    xyz: new Float32Array(xyz),
    width: new Float32Array(kind.length),
    kind: new Uint8Array(kind),
  });

  it('puts a cut on the blank where the machine turns to it', () => {
    // Z zero on the axis, so the surface is at 20: a cut at Z 18 is 2 inside.
    const moves = reply([10, 0, 18, 10, quarter, 18], [MOVE_KIND.feed, MOVE_KIND.feed]);
    const { cut } = wrappedToolpathLines(moves, job('x'), 20);
    // Starts on top of the blank, 2 under the surface, over X 10.
    expect(cut[0]).toBeCloseTo(10, 5);
    expect(cut[1]).toBeCloseTo(-2, 5);
    expect(cut[2]).toBeCloseTo(0, 5);
    // A quarter turn on: level with the axis (20 under the top), 18 from it,
    // on the far side, where flat Y lies.
    const end = cut.length - 3;
    expect(cut[end]).toBeCloseTo(10, 5);
    expect(cut[end + 1]).toBeCloseTo(-20, 5);
    expect(cut[end + 2]).toBeCloseTo(-18, 4);
    // 90 degrees in pieces of at most 5: 18 pieces of two ends each.
    expect(cut.length).toBe(18 * 6);
  });

  it('keeps every piece of a long turn at its distance from the axis', () => {
    const moves = reply([0, 0, 20, 30, 4 * quarter, 20], [MOVE_KIND.feed, MOVE_KIND.feed]);
    const { cut } = wrappedToolpathLines(moves, job('x'), 20);
    expect(cut.length).toBe(72 * 6);
    for (let i = 0; i < cut.length; i += 3) {
      const up = (cut[i + 1] ?? 0) + 20;
      const side = cut[i + 2] ?? 0;
      expect(Math.hypot(up, side)).toBeCloseTo(20, 3);
    }
    // A straight flat move is a helix: X moves on steadily as it turns.
    expect(cut[0]).toBeCloseTo(0, 5);
    expect(cut[cut.length - 3]).toBeCloseTo(30, 4);
  });

  it('wraps flat X round a blank along Y', () => {
    const moves = reply([0, 30, 20, quarter, 30, 20], [MOVE_KIND.feed, MOVE_KIND.feed]);
    const { cut } = wrappedToolpathLines(moves, job('y'), 20);
    const end = cut.length - 3;
    expect(cut[end]).toBeCloseTo(20, 4);
    expect(cut[end + 1]).toBeCloseTo(-20, 4);
    expect(cut[end + 2]).toBeCloseTo(-30, 5);
  });

  it('takes Z from a surface zero and leaves hidden moves out', () => {
    const moves = reply(
      [0, 0, 5, 0, 0, -3, 0, quarter, -3],
      [MOVE_KIND.rapid, MOVE_KIND.plunge, MOVE_KIND.feed],
    );
    const surface = wrappedToolpathLines(moves, job('x'), 0, {
      cut: false,
      rapid: true,
      plunge: true,
      retract: true,
      join: true,
    });
    expect(surface.cut.length).toBe(0);
    // Z -3 under a surface zero is 3 inside the blank; a plunge turns nothing.
    expect(Array.from(surface.plunge)).toEqual([0, 5, -0, 0, -3, -0]);
  });
});
