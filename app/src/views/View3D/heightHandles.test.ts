import { describe, expect, it } from 'vitest';

import {
  dragHeights,
  heightOf,
  heightPatch,
  heightsOf,
  riseAlong,
  typedHeights,
  type Heights,
} from './heightHandles';

const START: Heights = { base: 2, shape: 8, scale: 1 };

describe('3D height handles', () => {
  it('reads the heights of a component', () => {
    expect(
      heightsOf({
        id: 1,
        name: 'a',
        visible: true,
        combine: 'add',
        heightScale: 0.5,
        level: 0,
        maxHeight: 6,
        baseHeight: 1,
      }),
    ).toEqual({ base: 1, shape: 5, scale: 0.5 });
  });

  it('stands the handles on the post in order', () => {
    expect(heightOf('base', START)).toBe(2);
    expect(heightOf('shape', START)).toBe(10);
    expect(heightOf('total', START)).toBeGreaterThan(10);
  });

  it('raises the base, never below zero', () => {
    expect(dragHeights('base', START, 3)).toEqual({ base: 5, shape: 8, scale: 1 });
    expect(dragHeights('base', START, -5).base).toBe(0);
  });

  it('scales the shape above the base', () => {
    expect(dragHeights('shape', START, 8)).toEqual({ base: 2, shape: 16, scale: 2 });
    expect(dragHeights('shape', START, -100).scale).toBeCloseTo(0.01);
  });

  it('scales base and shape together with the total handle', () => {
    const now = dragHeights('total', START, 10);
    expect(now.base).toBeCloseTo(4);
    expect(now.shape).toBeCloseTo(16);
    expect(now.scale).toBeCloseTo(2);
  });

  it('takes typed values', () => {
    expect(typedHeights('base', START, 0)).toEqual({ base: 0, shape: 8, scale: 1 });
    expect(typedHeights('shape', START, 50)).toEqual({ base: 2, shape: 4, scale: 0.5 });
    expect(typedHeights('total', START, 5)?.base).toBeCloseTo(1);
    expect(typedHeights('base', START, -1)).toBeNull();
    expect(typedHeights('shape', START, 0)).toBeNull();
  });

  it('patches only what changed', () => {
    expect(heightPatch(START, { ...START, base: 3 })).toEqual({ baseHeight: 3 });
    expect(heightPatch(START, START)).toEqual({});
  });

  it('measures a pointer move along the projected post', () => {
    // One millimetre up is 4 pixels up the screen.
    expect(riseAlong({ x: 0, y: 0 }, { x: 0, y: -4 }, { x: 5, y: 10 }, { x: 9, y: -10 })).toBe(5);
    expect(riseAlong({ x: 0, y: 0 }, { x: 0, y: 0 }, { x: 0, y: 0 }, { x: 0, y: 9 })).toBe(0);
  });
});
