import { describe, expect, it } from 'vitest';

import type { EntitySummary, Path } from '../../bridge/document';
import { circlePath, polylinePath } from '../../lib/geometry';
import {
  addToAllNodes,
  attachSection,
  deleteStation,
  initialSweepState,
  moveStationEnd,
  railsFromSelection,
  reverseRail,
  sectionsToUse,
  setStart,
  spansMatch,
  stationAt,
  stationEndAt,
  swapRails,
  sweepArgs,
  sweepOverlays,
  toggleSmooth,
  validateSweep,
  withRails,
} from './model';
import { nearestOnRail, pointAt, railOf, startAt, type Rail } from './rails';

function rail(path: Path, reverse = false): Rail {
  const r = railOf(path, reverse, null);
  if (r === null) {
    throw new Error('no rail');
  }
  return r;
}

const lower = polylinePath([
  { x: 0, y: 0 },
  { x: 40, y: 0 },
]);
const upper = polylinePath([
  { x: 0, y: 10 },
  { x: 40, y: 10 },
]);

function entity(id: number, closed: boolean): EntitySummary {
  return { id, kind: 'path', closed, layer: 1 } as EntitySummary;
}

describe('two-rail sweep rails', () => {
  it('measures a rail, reverses it and starts a closed one where asked', () => {
    const r = rail(lower);
    expect(pointAt(r, 0.25)).toEqual({ x: 10, y: 0 });
    expect(nearestOnRail(r, { x: 30, y: 3 }).f).toBeCloseTo(0.75);
    expect(pointAt(rail(lower, true), 0)).toEqual({ x: 40, y: 0 });
    const loop = railOf(circlePath(0, 0, 10), false, { x: 0, y: 20 });
    expect(loop?.closed).toBe(true);
    const start = loop === null ? null : pointAt(loop, 0);
    expect(start?.x).toBeCloseTo(0);
    expect(start?.y).toBeCloseTo(10, 1);
    const square = startAt(
      [
        { x: 0, y: 0 },
        { x: 10, y: 0 },
        { x: 10, y: 10 },
      ],
      { x: 5, y: -1 },
    );
    expect(square[0]).toEqual({ x: 5, y: 0 });
    expect(square).toHaveLength(4);
  });
});

describe('two-rail sweep form', () => {
  const rails: [Rail, Rail] = [rail(lower), rail(upper)];
  const base = withRails(initialSweepState('mm'), [1, 2]);

  it('takes two vectors, open or closed, as the rails in selection order', () => {
    const entities = [entity(1, false), entity(2, true), entity(3, false)];
    expect(railsFromSelection([2, 1], entities)).toEqual([2, 1]);
    expect(railsFromSelection([1], entities)).toBeNull();
    expect(railsFromSelection([1, 2, 3], entities)).toBeNull();
  });

  it('puts the first section across both ends, later ones where clicked', () => {
    const first = attachSection(base, rails, 0, 7, { x: 12, y: 0 });
    expect(first.stations.map((s) => [s.a, s.b])).toEqual([
      [
        { x: 0, y: 0 },
        { x: 0, y: 10 },
      ],
      [
        { x: 40, y: 0 },
        { x: 40, y: 10 },
      ],
    ]);
    const second = attachSection(first, rails, 1, 8, { x: 20, y: 10 });
    expect(second.stations.map((s) => s.section)).toEqual([7, 8, 7]);
    expect(second.stations[1]?.a).toEqual({ x: 20, y: 0 });
    expect(sectionsToUse(second, [])).toEqual([7, 8]);
    const args = sweepArgs(second, rails, [], 'mm');
    expect(args?.sections).toEqual([7, 8]);
    expect(args?.params.stations).toEqual([
      { section: 0, a: 0, b: 0, smooth: true },
      { section: 1, a: 0.5, b: 0.5, smooth: true },
      { section: 0, a: 1, b: 1, smooth: true },
    ]);
  });

  it('uses a lone selected open vector when nothing is attached', () => {
    expect(sectionsToUse(base, [1, 5])).toEqual([5]);
    expect(sectionsToUse(base, [5, 6])).toEqual([]);
    const args = sweepArgs(base, rails, [5], 'mm');
    expect(args).toMatchObject({ rails: [1, 2], sections: [5] });
    expect(args?.params.stations).toEqual([]);
    expect(sweepArgs(base, null, [5], 'mm')).toBeNull();
  });

  it('drags section ends along their rails, deletes and creases them', () => {
    const state = attachSection(base, rails, 0, 7, { x: 0, y: 0 });
    expect(stationEndAt(state, { x: 39, y: 9.5 }, 2)).toEqual({ index: 1, end: 'b' });
    expect(stationAt(state, { x: 0.5, y: 5 }, 1)).toBe(0);
    const moved = moveStationEnd(state, rails, 1, 'b', { x: 30, y: 14 });
    expect(moved.stations[1]?.b).toEqual({ x: 30, y: 10 });
    expect(sweepArgs(moved, rails, [], 'mm')?.params.stations?.[1]).toMatchObject({
      a: 1,
      b: 0.75,
    });
    expect(toggleSmooth(state, 0).stations[0]?.smooth).toBe(false);
    expect(deleteStation(state, 0).stations).toHaveLength(1);
  });

  it('adds the section at every node pair when the node counts match', () => {
    const zig = polylinePath([
      { x: 0, y: 0 },
      { x: 20, y: 0 },
      { x: 40, y: 0 },
    ]);
    const zag = polylinePath([
      { x: 0, y: 10 },
      { x: 10, y: 10 },
      { x: 40, y: 10 },
    ]);
    const nodes = addToAllNodes(base, [rail(zig), rail(zag)], 4);
    expect(nodes?.stations.map((s) => [s.a.x, s.b.x])).toEqual([
      [0, 0],
      [20, 10],
      [40, 40],
    ]);
    expect(addToAllNodes(base, [rail(zig), rail(upper)], 4)).toBeNull();
  });

  it('swaps, reverses and restarts the rails', () => {
    const state = attachSection(base, rails, 0, 7, { x: 0, y: 0 });
    const swapped = swapRails(reverseRail(state, 0));
    expect(swapped.rails).toEqual([2, 1]);
    expect(swapped.reverse).toEqual([false, true]);
    expect(swapped.stations[0]?.a).toEqual({ x: 0, y: 10 });
    expect(setStart(state, 1, { x: 3, y: 4 }).starts).toEqual([null, { x: 3, y: 4 }]);
    expect(withRails(swapped, [5, 6]).stations).toEqual([]);
  });

  it('checks the exact height only when it is on, and spans only when they match', () => {
    const state = { ...base, height: '0' };
    expect(validateSweep(state, 'mm')).toEqual({});
    expect(validateSweep({ ...state, exactHeight: true }, 'mm').height).toBeDefined();
    const two = polylinePath([
      { x: 0, y: 0 },
      { x: 1, y: 1 },
      { x: 2, y: 0 },
    ]);
    expect(spansMatch([two, two])).toBe(true);
    expect(spansMatch([two, lower])).toBe(false);
    expect(spansMatch([two, undefined])).toBe(false);
  });

  it('draws the rails red and green with a start marker, and the sections', () => {
    const state = attachSection(base, rails, 0, 7, { x: 0, y: 0 });
    const overlays = sweepOverlays(state, rails, [lower, upper], 0.1);
    expect(overlays.filter((o) => o.tone === 'first').length).toBeGreaterThan(1);
    expect(overlays.filter((o) => o.tone === 'second' && o.fill === true)).toHaveLength(2);
    expect(overlays.filter((o) => o.tone === 'accent' && o.dashed === true)).toHaveLength(11);
    expect(sweepOverlays(state, null, [], 0.1)).toEqual([]);
  });
});
