import { describe, expect, it } from 'vitest';

import {
  convertTransformUnits,
  initialTransformState,
  setLinked,
  setTransformField,
  transformedCorners,
  transformParams,
  transformPreview,
  validateTransform,
  type Bbox,
} from './model';

const bbox: Bbox = [0, 0, 20, 10];
const mm = initialTransformState('mm');

function close(points: Array<{ x: number; y: number }>): Array<[number, number]> {
  return points.map((p) => [Number(p.x.toFixed(9)) + 0, Number(p.y.toFixed(9)) + 0]);
}

describe('component transform model', () => {
  it('starts as the identity about the component centre', () => {
    expect(transformParams(mm, 'mm')).toEqual({
      offset: { x: 0, y: 0 },
      scaleX: 1,
      scaleY: 1,
      rotateDeg: 0,
    });
    expect(validateTransform(mm, 'mm')).toEqual({});
  });

  it('builds the kernel parameters from the fields', () => {
    const state = {
      ...mm,
      dx: '1',
      dy: '-2',
      scaleX: '50',
      scaleY: '200',
      angle: '90',
      pivotAtCentre: false,
      pivotX: '0.5',
      pivotY: '1',
    };
    expect(transformParams(state, 'inch')).toEqual({
      offset: { x: 25.4, y: -50.8 },
      scaleX: 0.5,
      scaleY: 2,
      rotateDeg: 90,
      centre: { x: 12.7, y: 25.4 },
    });
  });

  it('refuses scales at or below zero and unreadable fields', () => {
    const errors = validateTransform({ ...mm, scaleX: '0', scaleY: '-5', angle: 'x' }, 'mm');
    expect(Object.keys(errors).sort()).toEqual(['angle', 'scaleX', 'scaleY']);
    expect(transformParams({ ...mm, scaleX: '0' }, 'mm')).toBeNull();
  });

  it('checks the pivot only when a point is typed', () => {
    expect(validateTransform({ ...mm, pivotX: 'x' }, 'mm')).toEqual({});
    expect(validateTransform({ ...mm, pivotAtCentre: false, pivotX: 'x' }, 'mm')).toHaveProperty(
      'pivotX',
    );
  });

  it('links the two scales', () => {
    const linked = setTransformField(mm, 'scaleX', '150');
    expect([linked.scaleX, linked.scaleY]).toEqual(['150', '150']);
    const free = setTransformField(setLinked(mm, false), 'scaleY', '80');
    expect([free.scaleX, free.scaleY]).toEqual(['100', '80']);
    expect(setLinked(free, true).scaleY).toBe('100');
  });

  it('converts lengths between units and leaves percent alone', () => {
    const inch = convertTransformUnits({ ...mm, dx: '25.4', scaleX: '50' }, 'mm', 'inch');
    expect(inch.dx).toBe('1');
    expect(inch.scaleX).toBe('50');
  });

  it('maps the box corners like the kernel', () => {
    const params = { offset: { x: 5, y: 0 }, scaleX: 2, scaleY: 1, rotateDeg: 90 };
    // Centre (10, 5): scaled half extents 20 by 5, turned a quarter, moved 5 in X.
    expect(close(transformedCorners(bbox, params))).toEqual([
      [20, -15],
      [20, 25],
      [10, 25],
      [10, -15],
    ]);
    const about = { ...params, rotateDeg: 0, offset: { x: 0, y: 0 }, centre: { x: 0, y: 0 } };
    expect(close(transformedCorners(bbox, about))).toEqual([
      [0, 0],
      [40, 0],
      [40, 10],
      [0, 10],
    ]);
  });

  it('previews a closed box and a pivot cross', () => {
    const [box, pivot] = transformPreview({ ...mm, dx: '3' }, 'mm', bbox);
    expect(box?.subpaths[0]?.closed).toBe(true);
    expect(box?.subpaths[0]?.start).toEqual({ x: 3, y: 0 });
    expect(box?.subpaths[0]?.segments.map((s) => s.to)).toEqual([
      { x: 23, y: 0 },
      { x: 23, y: 10 },
      { x: 3, y: 10 },
    ]);
    expect(pivot?.subpaths).toHaveLength(2);
    expect(transformPreview(mm, 'mm', undefined)).toEqual([]);
    expect(transformPreview({ ...mm, scaleX: '0' }, 'mm', bbox)).toEqual([]);
  });
});
