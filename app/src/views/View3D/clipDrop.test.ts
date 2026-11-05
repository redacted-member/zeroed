import { describe, expect, it } from 'vitest';

import { clipDropPoint, ndcIn } from './clipDrop';

const rect = { left: 100, top: 50, width: 400, height: 200 };

describe('a clip dropped on the 3D view', () => {
  it('maps the pointer to device coordinates', () => {
    expect(ndcIn(rect, 300, 150)).toEqual({ x: 0, y: -0 });
    expect(ndcIn(rect, 100, 50)).toEqual({ x: -1, y: 1 });
    expect(ndcIn(rect, 500, 250)).toEqual({ x: 1, y: -1 });
  });

  it('lands on the model surface under the pointer', () => {
    const asked: number[][] = [];
    const point = clipDropPoint(
      {
        pick: (x, y) => {
          asked.push([x, y]);
          return { x: 12, y: 34 };
        },
        pickPlane: () => ({ x: 0, y: 0 }),
      },
      rect,
      500,
      50,
    );
    expect(point).toEqual({ x: 12, y: 34 });
    expect(asked).toEqual([[1, 1]]);
  });

  it('falls back to the top of the material, then to nothing', () => {
    const planes: number[] = [];
    const picker = {
      pick: () => null,
      pickPlane: (_x: number, _y: number, sceneY: number) => {
        planes.push(sceneY);
        return { x: 5, y: 6 };
      },
    };
    expect(clipDropPoint(picker, rect, 200, 100)).toEqual({ x: 5, y: 6 });
    expect(planes).toEqual([0]);
    expect(clipDropPoint({ pick: () => null, pickPlane: () => null }, rect, 1, 1)).toBeNull();
    expect(clipDropPoint(picker, { ...rect, width: 0 }, 1, 1)).toBeNull();
  });
});
