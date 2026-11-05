import { describe, expect, it } from 'vitest';

import {
  frameFor,
  layoutFrame,
  NO_TRANSFORM,
  pivotAfter,
  type TransformFrame,
} from './handleFrame';

const COMPONENT = {
  id: 7,
  name: 'a',
  visible: true,
  combine: 'add' as const,
  heightScale: 1,
  level: 0,
  maxHeight: 10,
  baseHeight: 2,
  bbox: [0, 0, 100, 50] as [number, number, number, number],
};

describe('transform handle frame', () => {
  it('keeps the rotation centre for the same component', () => {
    const first = frameFor(COMPONENT, 5, null);
    expect(first?.planeY).toBe(15);
    expect(first?.heights).toEqual({ base: 2, shape: 8, scale: 1 });
    const kept = frameFor(COMPONENT, 5, {
      ...(first as TransformFrame),
      pivot: { x: 3, y: 4 },
      centreMode: true,
    });
    expect(kept?.pivot).toEqual({ x: 3, y: 4 });
    expect(kept?.centreMode).toBe(true);
    expect(frameFor({ ...COMPONENT, id: 8 }, 5, kept)?.pivot).toBeNull();
  });

  it('carries the rotation centre along with a move', () => {
    const frame = frameFor(COMPONENT, 0, null) as TransformFrame;
    const moved = {
      ...frame,
      pivot: { x: 10, y: 10 },
      params: { ...NO_TRANSFORM, offset: { x: 5, y: 0 } },
    };
    expect(pivotAfter(moved)).toEqual({ x: 15, y: 10 });
  });

  it('lays out box and height handles, the middle on the rotation centre', () => {
    const frame = {
      ...(frameFor(COMPONENT, 0, null) as TransformFrame),
      pivot: { x: 20, y: 30 },
      centreMode: true,
    };
    const shown = layoutFrame((x, y, z) => ({ x, y: y + z * 1000 }), frame);
    expect(shown?.outline).toHaveLength(4);
    const at = (kind: string) => shown?.handles.find((h) => h.handle.kind === kind)?.at;
    expect(at('move')).toEqual({ x: 20, y: 10030 });
    expect(at('base')).toEqual({ x: 112, y: 2000 });
    expect(at('shape')).toEqual({ x: 112, y: 10000 });
    expect(at('total')?.y).toBeGreaterThan(10000 + 11000);
    expect(shown?.post?.from).toEqual({ x: 112, y: 0 });
    expect(layoutFrame(() => null, frame)).toBeNull();
  });
});
