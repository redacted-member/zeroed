import { describe, expect, it } from 'vitest';

import type { Bbox } from '../../bridge/document';
import {
  distortArgs,
  dragNode,
  isBent,
  nodeMarkers,
  pickNode,
  startEnvelope,
  targetBox,
  distortTargetProblem,
  curveArgs,
  fromFrame,
  modeFromSelection,
  splitSelection,
  storedEnvelope,
  toFrame,
} from './model';

const box: Bbox = [0, 0, 100, 50];

function corners(): NonNullable<ReturnType<typeof startEnvelope>> {
  const envelope = startEnvelope(box);
  if (envelope === null) {
    throw new Error('no envelope');
  }
  return envelope;
}

describe('distort form model', () => {
  it('starts as the four corners joined by straight spans', () => {
    const envelope = corners();
    expect(envelope.bottom).toHaveLength(2);
    expect(envelope.top).toHaveLength(2);
    expect(envelope.left).toHaveLength(2);
    expect(envelope.right).toHaveLength(2);
    expect(startEnvelope(null)).toBeNull();
  });

  it('builds the box from the selected vectors and components', () => {
    expect(targetBox(null, null)).toBeNull();
    expect(targetBox([0, 0, 10, 0], null)).toBeNull();
    expect(targetBox([0, 0, 10, 10], null)).toEqual([0, 0, 10, 10]);
    expect(targetBox([0, 0, 10, 10], [5, -5, 20, 5])).toEqual([0, -5, 20, 10]);
    expect(targetBox(null, [5, -5, 20, 5])).toEqual([5, -5, 20, 5]);
  });

  it('picks and drags the nearest node, a corner moving both its edges', () => {
    const start = corners();
    expect(pickNode(start, { x: 30, y: 30 }, 2)).toBeNull();
    const node = pickNode(start, { x: 99, y: 49 }, 2);
    expect(node).toEqual({ edge: 'top', index: 1 });
    if (node === null) {
      throw new Error('no node');
    }
    expect(isBent(start, start)).toBe(false);
    const bent = dragNode(start, node, { x: 110, y: 70 });
    expect(isBent(bent, start)).toBe(true);
    expect(bent.right?.[1]).toEqual({ x: 110, y: 70 });
    expect(nodeMarkers(bent, 2)).toHaveLength(4);
  });

  it('sends every edge, its line spans and the box', () => {
    const start = corners();
    expect(distortArgs(start, [4], [9])).toEqual({
      ids: [4],
      componentIds: [9],
      bbox: box,
      envelope: {
        bottom: start.bottom,
        top: start.top,
        left: start.left,
        right: start.right,
        // The box envelope starts as four line spans.
        lines: { bottom: [true], top: [true], left: [true], right: [true] },
      },
    });
  });

  it('refuses vectors and components together, and several components', () => {
    expect(distortTargetProblem(2, 0)).toBeNull();
    expect(distortTargetProblem(0, 1)).toBeNull();
    expect(distortTargetProblem(1, 1)).toBe('mixed');
    expect(distortTargetProblem(0, 2)).toBe('several');
  });

  it('reads the mode from the selection order and splits off the curves', () => {
    const open = (id: number) => id >= 10;
    expect(modeFromSelection([1, 2], open)).toBe('box');
    expect(modeFromSelection([1, 10], open)).toBe('curve');
    expect(modeFromSelection([1, 10, 11], open)).toBe('twoCurves');
    expect(modeFromSelection([10], open)).toBe('box');
    expect(modeFromSelection([10, 11], open)).toBe('curve');
    expect(splitSelection([1, 2, 10, 11], 'twoCurves')).toEqual({
      objects: [1, 2],
      curves: [10, 11],
    });
    expect(splitSelection([1, 10], 'curve')).toEqual({ objects: [1], curves: [10] });
    expect(splitSelection([1, 10], 'box')).toEqual({ objects: [1, 10], curves: [] });
    expect(curveArgs('curve', [1], [10], [], 30)).toEqual({
      ids: [1],
      componentIds: [],
      mode: 'curve',
      curves: [10],
      angle: 30,
    });
  });

  it('turns points into and out of a rotated frame', () => {
    const p = toFrame({ x: 0, y: 10 }, 90);
    expect(p.x).toBeCloseTo(10);
    expect(p.y).toBeCloseTo(0);
    const back = fromFrame(p, 90);
    expect(back.x).toBeCloseTo(0);
    expect(back.y).toBeCloseTo(10);
    expect(
      distortArgs(
        storedEnvelope({
          ids: [1],
          mode: 'box',
          angle: 90,
          bbox: box,
          bottom: [
            { x: 0, y: 0 },
            { x: 100, y: 0 },
          ],
          top: [
            { x: 0, y: 50 },
            { x: 100, y: 50 },
          ],
        }),
        [1],
        [],
        90,
      ).angle,
    ).toBe(90);
  });
});
