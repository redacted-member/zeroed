import { describe, expect, it } from 'vitest';

import type { Bbox } from '../bridge/document';
import {
  cornerEnvelope,
  curveAt,
  deleteEdgeNode,
  envelopeKeyEdit,
  fromEnvelopeFrame,
  insertEdgeNode,
  nearestEdge,
  toEnvelopeFrame,
  withSides,
  distortPreview,
  envelopeNodes,
  envelopeOutline,
  identityEnvelope,
  mapPoint,
  mapUv,
  moveNode,
  setSpanLine,
  type Envelope,
} from './envelope';
import { rectPath } from './geometry';

const box: Bbox = [10, 20, 110, 70];

function arched(lift: number): Envelope {
  return moveNode(identityEnvelope(box, 3), 'top', 1, { x: 60, y: 70 + lift });
}

describe('envelope maths', () => {
  it('passes through its nodes and keeps even straight nodes straight', () => {
    const nodes = [
      { x: 0, y: 0 },
      { x: 1, y: 4 },
      { x: 3, y: -2 },
    ];
    expect(curveAt(nodes, 0.5).x).toBeCloseTo(1, 12);
    expect(curveAt(nodes, 0.5).y).toBeCloseTo(4, 12);
    const straight = [
      { x: 0, y: 0 },
      { x: 3, y: 1 },
      { x: 6, y: 2 },
    ];
    expect(curveAt(straight, 0.25).x).toBeCloseTo(1.5, 12);
    expect(curveAt(straight, 0.25).y).toBeCloseTo(0.5, 12);
  });

  it('maps every point to itself while unbent', () => {
    const envelope = identityEnvelope(box, 4, 3);
    for (const p of [
      { x: 10, y: 20 },
      { x: 37.5, y: 61.25 },
      { x: 110, y: 70 },
    ]) {
      const q = mapPoint(envelope, p);
      expect(q.x).toBeCloseTo(p.x, 9);
      expect(q.y).toBeCloseTo(p.y, 9);
    }
  });

  it('matches the kernel on an arch', () => {
    const envelope = arched(20);
    expect(mapUv(envelope, 0.5, 1).y).toBeCloseTo(90, 9);
    expect(mapUv(envelope, 0.5, 0).y).toBeCloseTo(20, 9);
    expect(mapUv(envelope, 0.5, 0.5).y).toBeCloseTo(55, 9);
  });

  it('keeps shared corners shared when a node moves', () => {
    const envelope = identityEnvelope(box, 3, 3);
    const moved = moveNode(envelope, 'bottom', 0, { x: 0, y: 0 });
    expect(moved.left?.[0]).toEqual({ x: 0, y: 0 });
    const top = moveNode(envelope, 'right', 2, { x: 120, y: 80 });
    expect(top.top[2]).toEqual({ x: 120, y: 80 });
    expect(envelope.bottom[0]).toEqual({ x: 10, y: 20 });
    expect(moveNode(envelope, 'top', 9, { x: 0, y: 0 })).toBe(envelope);
  });

  it('lists corners once', () => {
    expect(envelopeNodes(identityEnvelope(box, 3, 3))).toHaveLength(8);
    expect(envelopeNodes(identityEnvelope(box, 3))).toHaveLength(6);
  });

  it('previews a bent rectangle and an outline', () => {
    const [bent] = distortPreview([rectPath(10, 20, 100, 50)], arched(20), 0.1);
    const ys = (bent?.subpaths[0]?.segments ?? []).map((s) => s.to.y);
    expect(Math.max(...ys)).toBeCloseTo(90, 6);
    const outline = envelopeOutline(arched(20), 8);
    expect(outline.subpaths[0]?.closed).toBe(true);
    expect(outline.subpaths[0]?.segments).toHaveLength(31);
  });
});

describe('node editing an envelope', () => {
  it('finds the edge under a point', () => {
    const envelope = cornerEnvelope(box);
    expect(nearestEdge(envelope, { x: 60, y: 20.5 }, 1)).toBe('bottom');
    expect(nearestEdge(envelope, { x: 109.5, y: 40 }, 1)).toBe('right');
    expect(nearestEdge(envelope, { x: 60, y: 45 }, 1)).toBeNull();
  });

  it('inserts a node without changing the shape, and deletes it again', () => {
    const bent = moveNode(cornerEnvelope(box), 'top', 1, { x: 120, y: 90 });
    const more = insertEdgeNode(bent, 'left');
    expect(more.left).toHaveLength(3);
    for (const [u, v] of [
      [0.25, 0.5],
      [0.5, 0.5],
      [0.9, 0.1],
    ] as const) {
      const a = mapUv(bent, u, v);
      const b = mapUv(more, u, v);
      expect(b.x).toBeCloseTo(a.x, 9);
      expect(b.y).toBeCloseTo(a.y, 9);
    }
    const fewer = deleteEdgeNode(more, 'left', 1);
    expect(fewer.left).toHaveLength(2);
    // Corners stay.
    expect(deleteEdgeNode(more, 'bottom', 0)).toBe(more);
  });

  it('edits with the I and D keys', () => {
    const envelope = cornerEnvelope(box);
    const inserted = envelopeKeyEdit(envelope, 'i', { x: 60, y: 20.2 }, 1, null);
    expect(inserted?.bottom).toHaveLength(3);
    if (inserted === null) {
      throw new Error('inserted');
    }
    const middle = inserted.bottom[1] ?? { x: 0, y: 0 };
    expect(middle.x).toBeCloseTo(60, 9);
    expect(envelopeKeyEdit(inserted, 'd', middle, 1, null)?.bottom).toHaveLength(2);
    expect(
      envelopeKeyEdit(inserted, 'Delete', null, 1, { edge: 'bottom', index: 1 })?.bottom,
    ).toHaveLength(2);
    expect(envelopeKeyEdit(envelope, 'd', { x: 10, y: 20 }, 1, null)).toBeNull();
    expect(envelopeKeyEdit(envelope, 'q', { x: 60, y: 20 }, 1, null)).toBeNull();
  });

  it('gives an envelope without sides straight side lists', () => {
    const sided = withSides(identityEnvelope(box, 3));
    expect(sided.left).toEqual([
      { x: 10, y: 20 },
      { x: 10, y: 70 },
    ]);
    const p = toEnvelopeFrame({ x: 3, y: 4 }, 30);
    const back = fromEnvelopeFrame(p, 30);
    expect(back.x).toBeCloseTo(3, 9);
    expect(back.y).toBeCloseTo(4, 9);
  });

  it('starts with four line spans that stay straight when a split node moves', () => {
    const envelope = cornerEnvelope(box);
    expect(envelope.lines).toEqual({ bottom: [true], top: [true], left: [true], right: [true] });
    const split = envelopeKeyEdit(envelope, 'i', { x: 60, y: 70.2 }, 1, null);
    if (split === null) {
      throw new Error('split');
    }
    expect(split.lines?.top).toEqual([true, true]);
    // Pull the new top node up 20: the top runs straight up to it, a V.
    const peaked = moveNode(split, 'top', 1, { x: 60, y: 90 });
    const quarter = mapUv(peaked, 0.25, 1);
    expect(quarter.x).toBeCloseTo(35, 9);
    expect(quarter.y).toBeCloseTo(80, 9);
    // As a curve (C over the span) the same nodes bulge above the line.
    const curved = envelopeKeyEdit(peaked, 'c', { x: 35, y: 80 }, 1, null);
    expect(curved?.lines?.top).toEqual([false, true]);
    if (curved === null) {
      throw new Error('curved');
    }
    expect(mapUv(curved, 0.25, 1).y).toBeGreaterThan(80.5);
    // L makes it a line again; asking for what it already is does nothing.
    const back = envelopeKeyEdit(curved, 'l', { x: 35, y: 81 }, 1, null);
    expect(back?.lines?.top).toEqual([true, true]);
    expect(envelopeKeyEdit(peaked, 'l', { x: 35, y: 80 }, 1, null)).toBeNull();
  });

  it('joins two spans into a line only when both were lines', () => {
    const split = envelopeKeyEdit(cornerEnvelope(box), 'i', { x: 60, y: 20.2 }, 1, null);
    if (split === null) {
      throw new Error('split');
    }
    expect(deleteEdgeNode(split, 'bottom', 1).lines?.bottom).toEqual([true]);
    const mixed = setSpanLine(split, 'bottom', 0, false);
    expect(mixed.lines?.bottom).toEqual([false, true]);
    expect(deleteEdgeNode(mixed, 'bottom', 1).lines?.bottom).toBeUndefined();
    expect(setSpanLine(split, 'bottom', 5, false)).toBe(split);
  });
});
