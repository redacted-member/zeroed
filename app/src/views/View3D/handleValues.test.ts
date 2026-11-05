import { describe, expect, it } from 'vitest';

import { NO_TRANSFORM } from './handleFrame';
import {
  isHeightHandle,
  typedHeightValues,
  typedTransform,
  valueFields,
  type HandleState,
} from './handleValues';
import type { Bbox } from './transformHandles';

const BOX: Bbox = [0, 0, 100, 50];
const STATE: HandleState = {
  bbox: BOX,
  params: NO_TRANSFORM,
  pivot: null,
  centreMode: false,
  heights: { base: 1, shape: 9, scale: 1 },
  profile: null,
};

describe('handle value box', () => {
  it('shows the values each handle changes', () => {
    expect(valueFields({ kind: 'move' }, STATE).map((f) => f.value)).toEqual([50, 25]);
    const scaled = { ...STATE, params: { ...NO_TRANSFORM, scaleX: 2 } };
    expect(valueFields({ kind: 'corner', index: 2 }, scaled).map((f) => f.value)).toEqual([
      200, 50,
    ]);
    expect(valueFields({ kind: 'edge', index: 1 }, STATE).map((f) => f.key)).toEqual(['width']);
    expect(valueFields({ kind: 'edge', index: 0 }, STATE).map((f) => f.key)).toEqual(['height']);
    expect(valueFields({ kind: 'total' }, STATE)[0]?.value).toBe(10);
    expect(valueFields({ kind: 'shape' }, STATE)[0]).toMatchObject({ value: 100, unit: 'percent' });
    const centre = { ...STATE, centreMode: true, pivot: { x: 10, y: 20 } };
    expect(valueFields({ kind: 'move' }, centre).map((f) => f.value)).toEqual([10, 20]);
  });

  it('turns typed values into a transform', () => {
    expect(typedTransform({ kind: 'move' }, BOX, null, false, { x: 60, y: 20 })).toEqual({
      type: 'transform',
      params: { ...NO_TRANSFORM, offset: { x: 10, y: -5 } },
    });
    expect(
      typedTransform({ kind: 'corner', index: 2 }, BOX, null, false, { width: 50, height: 50 }),
    ).toEqual({
      type: 'transform',
      params: { ...NO_TRANSFORM, scaleX: 0.5, scaleY: 1, centre: { x: 0, y: 0 } },
    });
    expect(typedTransform({ kind: 'edge', index: 3 }, BOX, null, false, { width: 200 })).toEqual({
      type: 'transform',
      params: { ...NO_TRANSFORM, scaleX: 2, centre: { x: 100, y: 25 } },
    });
    expect(typedTransform({ kind: 'rotate' }, BOX, { x: 5, y: 5 }, false, { angle: 90 })).toEqual({
      type: 'transform',
      params: { ...NO_TRANSFORM, rotateDeg: 90, centre: { x: 5, y: 5 } },
    });
    expect(typedTransform({ kind: 'corner', index: 0 }, BOX, null, false, { width: 0 })).toBeNull();
  });

  it('moves the rotation centre in centre mode', () => {
    expect(typedTransform({ kind: 'move' }, BOX, { x: 1, y: 1 }, true, { x: 7 })).toEqual({
      type: 'pivot',
      pivot: { x: 7, y: 1 },
    });
  });

  it('turns typed heights into new heights', () => {
    expect(isHeightHandle({ kind: 'base' })).toBe(true);
    expect(isHeightHandle({ kind: 'move' })).toBe(false);
    expect(typedHeightValues('base', STATE.heights, { base: 3 })?.base).toBe(3);
    expect(typedHeightValues('shape', STATE.heights, {})).toBeNull();
  });
});
