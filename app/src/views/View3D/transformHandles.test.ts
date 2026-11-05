import { describe, expect, it } from 'vitest';

import {
  applyParams,
  dragParams,
  handlePoint,
  isIdentity,
  rotateHandleOf,
  type Bbox,
} from './transformHandles';

const BOX: Bbox = [0, 0, 100, 50];
const NO_KEYS = { alt: false, shift: false };

describe('3D transform handles', () => {
  it('places the handles on the box', () => {
    expect(handlePoint(BOX, { kind: 'move' })).toEqual({ x: 50, y: 25 });
    expect(handlePoint(BOX, { kind: 'corner', index: 2 })).toEqual({ x: 100, y: 50 });
    expect(handlePoint(BOX, { kind: 'edge', index: 1 })).toEqual({ x: 100, y: 25 });
    expect(rotateHandleOf(BOX).y).toBeGreaterThan(50);
  });

  it('moves, and along one axis with Alt', () => {
    const start = { x: 50, y: 25 };
    const move = dragParams({ kind: 'move' }, BOX, start, { x: 60, y: 28 }, NO_KEYS);
    expect(move.offset).toEqual({ x: 10, y: 3 });
    const axis = dragParams(
      { kind: 'move' },
      BOX,
      start,
      { x: 60, y: 28 },
      { alt: true, shift: false },
    );
    expect(axis.offset).toEqual({ x: 10, y: 0 });
  });

  it('scales from a corner about the opposite corner', () => {
    const corner = { kind: 'corner', index: 2 } as const;
    const p = dragParams(corner, BOX, { x: 100, y: 50 }, { x: 200, y: 100 }, NO_KEYS);
    expect(p.scaleX).toBeCloseTo(2);
    expect(p.scaleY).toBeCloseTo(2);
    expect(p.centre).toEqual({ x: 0, y: 0 });
    expect(applyParams(BOX, p, { x: 100, y: 50 })).toEqual({ x: 200, y: 100 });
    const free = dragParams(
      corner,
      BOX,
      { x: 100, y: 50 },
      { x: 200, y: 50 },
      { alt: true, shift: false },
    );
    expect(free.scaleX).toBeCloseTo(2);
    expect(free.scaleY).toBeCloseTo(1);
    const middle = dragParams(
      corner,
      BOX,
      { x: 100, y: 50 },
      { x: 150, y: 75 },
      { alt: false, shift: true },
    );
    expect(middle.centre).toEqual({ x: 50, y: 25 });
    expect(middle.scaleX).toBeCloseTo(2);
  });

  it('stretches along one axis from an edge, both with Shift', () => {
    const right = { kind: 'edge', index: 1 } as const;
    const p = dragParams(right, BOX, { x: 100, y: 25 }, { x: 50, y: 40 }, NO_KEYS);
    expect(p.scaleX).toBeCloseTo(0.5);
    expect(p.scaleY).toBe(1);
    const both = dragParams(
      right,
      BOX,
      { x: 100, y: 25 },
      { x: 50, y: 40 },
      { alt: false, shift: true },
    );
    expect(both.scaleY).toBeCloseTo(0.5);
    const top = dragParams(
      { kind: 'edge', index: 2 },
      BOX,
      { x: 50, y: 50 },
      { x: 50, y: 150 },
      NO_KEYS,
    );
    expect(top.scaleY).toBeCloseTo(3);
    const flipped = dragParams(right, BOX, { x: 100, y: 25 }, { x: -50, y: 25 }, NO_KEYS);
    expect(flipped.scaleX).toBeGreaterThan(0);
  });

  it('turns about the middle, in 15 degree steps with Alt', () => {
    const start = rotateHandleOf(BOX);
    const quarter = dragParams({ kind: 'rotate' }, BOX, start, { x: 0, y: 25 }, NO_KEYS);
    expect(quarter.rotateDeg).toBeCloseTo(90);
    const snapped = dragParams(
      { kind: 'rotate' },
      BOX,
      start,
      { x: 40, y: 80 },
      { alt: true, shift: false },
    );
    expect(snapped.rotateDeg % 15).toBeCloseTo(0);
    expect(isIdentity(dragParams({ kind: 'rotate' }, BOX, start, start, NO_KEYS))).toBe(true);
  });

  it('turns about a moved rotation centre', () => {
    const turn = dragParams({ kind: 'rotate' }, BOX, { x: 20, y: 0 }, { x: 10, y: 10 }, NO_KEYS, {
      x: 10,
      y: 0,
    });
    expect(turn.rotateDeg).toBeCloseTo(90);
    expect(turn.centre).toEqual({ x: 10, y: 0 });
  });
});
