import { describe, expect, it } from 'vitest';

import { applyMatrix } from './geometry';
import {
  alignMatrices,
  bboxAnchor,
  distributeMatrices,
  frameAnchor,
  fromFrame,
  selectionFrame,
  setFrameSizeMatrix,
  toFrame,
  mirrorMatrix,
  moveMatrix,
  rotateMatrix,
  setSizeMatrix,
  unionOf,
} from './transformMath';

const round = (v: number) => Math.round(v * 1000) / 1000;

describe('transform math', () => {
  const box: [number, number, number, number] = [10, 10, 30, 20];

  it('finds anchor points of a box', () => {
    expect(bboxAnchor(box, 'bottomLeft')).toEqual({ x: 10, y: 10 });
    expect(bboxAnchor(box, 'centre')).toEqual({ x: 20, y: 15 });
    expect(bboxAnchor(box, 'topRight')).toEqual({ x: 30, y: 20 });
    expect(bboxAnchor(box, 'top')).toEqual({ x: 20, y: 20 });
  });

  it('moves absolutely by anchor and relatively by delta', () => {
    const absolute = moveMatrix(box, 'centre', 'absolute', 0, 0);
    expect(applyMatrix(absolute, { x: 20, y: 15 })).toEqual({ x: 0, y: 0 });
    const relative = moveMatrix(box, 'centre', 'relative', 5, -5);
    expect(applyMatrix(relative, { x: 10, y: 10 })).toEqual({ x: 15, y: 5 });
  });

  it('sets the size about the anchor', () => {
    const m = setSizeMatrix(box, 'bottomLeft', 40, 5);
    expect(applyMatrix(m, { x: 10, y: 10 })).toEqual({ x: 10, y: 10 });
    expect(applyMatrix(m, { x: 30, y: 20 })).toEqual({ x: 50, y: 15 });
  });

  it('rotates about a pivot', () => {
    const m = rotateMatrix({ x: 10, y: 10 }, 90);
    const p = applyMatrix(m, { x: 20, y: 10 });
    expect(round(p.x)).toBe(10);
    expect(round(p.y)).toBe(20);
  });

  it('mirrors in place and across edges', () => {
    expect(applyMatrix(mirrorMatrix(box, 'horizontal'), { x: 10, y: 10 })).toEqual({
      x: 30,
      y: 10,
    });
    expect(applyMatrix(mirrorMatrix(box, 'vertical'), { x: 10, y: 10 })).toEqual({ x: 10, y: 20 });
    expect(applyMatrix(mirrorMatrix(box, 'left'), { x: 30, y: 15 })).toEqual({ x: -10, y: 15 });
    expect(applyMatrix(mirrorMatrix(box, 'top'), { x: 20, y: 10 })).toEqual({ x: 20, y: 30 });
  });

  it('aligns items to a target box', () => {
    const items = [
      { id: 1, bbox: [0, 0, 10, 10] as [number, number, number, number] },
      { id: 2, bbox: [50, 50, 70, 60] as [number, number, number, number] },
    ];
    const target: [number, number, number, number] = [100, 100, 200, 200];
    const left = alignMatrices(items, target, 'left');
    expect(left[0]?.matrix[4]).toBe(100);
    expect(left[1]?.matrix[4]).toBe(50);
    const centre = alignMatrices(items, target, 'centre');
    expect(centre[0]?.matrix[4]).toBe(145);
    expect(centre[0]?.matrix[5]).toBe(145);
    expect(unionOf(items.map((i) => i.bbox))).toEqual([0, 0, 70, 60]);
    // Outside: the far edge goes against the target's edge.
    expect(alignMatrices(items, target, 'outsideLeft')[0]?.matrix[4]).toBe(90);
    expect(alignMatrices(items, target, 'outsideRight')[0]?.matrix[4]).toBe(200);
    expect(alignMatrices(items, target, 'outsideTop')[0]?.matrix[5]).toBe(200);
    expect(alignMatrices(items, target, 'outsideBottom')[0]?.matrix[5]).toBe(90);
  });

  it('distributes by centres and by equal gaps', () => {
    const items = [
      { id: 1, bbox: [0, 0, 10, 10] as [number, number, number, number] },
      { id: 2, bbox: [12, 0, 22, 10] as [number, number, number, number] },
      { id: 3, bbox: [90, 0, 100, 10] as [number, number, number, number] },
    ];
    const centres = distributeMatrices(items, 'hCentre');
    expect(centres.find((m) => m.id === 2)?.matrix[4]).toBe(33);
    expect(centres.find((m) => m.id === 1)?.matrix[4]).toBe(0);
    expect(centres.find((m) => m.id === 3)?.matrix[4]).toBe(0);
    const gaps = distributeMatrices(items, 'hGap');
    // Total span 100, item widths 30, so two gaps of 35: item 2 starts at 45.
    expect(gaps.find((m) => m.id === 2)?.matrix[4]).toBe(33);
    const spaced = distributeMatrices(items, 'hGap', 5);
    expect(spaced.find((m) => m.id === 3)?.matrix[4]).toBe(-60);
    const vertical = distributeMatrices(
      items.map((i) => ({
        id: i.id,
        bbox: [i.bbox[1], i.bbox[0], i.bbox[3], i.bbox[2]] as [number, number, number, number],
      })),
      'vCentre',
    );
    expect(vertical.find((m) => m.id === 2)?.matrix[5]).toBe(33);
  });

  it('works along a rotated selection frame', () => {
    const frame = { angle: 30, extent: [0, 0, 20, 10] as [number, number, number, number] };
    const p = fromFrame(frame, { x: 20, y: 0 });
    expect([round(p.x), round(p.y)]).toEqual([round(20 * Math.cos(Math.PI / 6)), 10]);
    const back = toFrame(frame, p);
    expect([round(back.x), round(back.y)]).toEqual([20, 0]);
    expect(frameAnchor(frame, 'bottomLeft')).toEqual({ x: 0, y: 0 });
    // Doubling the frame width about the bottom left moves the far corner
    // twice as far along the rotated X axis.
    const m = setFrameSizeMatrix(frame, 'bottomLeft', 40, 10);
    const far = applyMatrix(m, p);
    const expected = fromFrame(frame, { x: 40, y: 0 });
    expect([round(far.x), round(far.y)]).toEqual([round(expected.x), round(expected.y)]);
    expect(selectionFrame({ ids: [1], bbox: [1, 2, 3, 4] })).toEqual({
      angle: 0,
      extent: [1, 2, 3, 4],
    });
    expect(selectionFrame({ ids: [], bbox: null })).toBeNull();
  });
});
