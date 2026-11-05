import { describe, expect, it } from 'vitest';

import { centreOnMaterial, drawingBounds } from './zoomAll';

const material = { minX: 0, minY: 0, maxX: 100, maxY: 50 };

describe('zoom to all', () => {
  it('takes in objects outside the material on visible layers only', () => {
    const objects = [
      { bbox: [-20, 10, 10, 20] as [number, number, number, number], layer: 1 },
      { bbox: [90, 40, 130, 80] as [number, number, number, number], layer: 2 },
      { bbox: null, layer: 1 },
    ];
    expect(drawingBounds(material, objects, new Set([1, 2]))).toEqual({
      minX: -20,
      minY: 0,
      maxX: 130,
      maxY: 80,
    });
    expect(drawingBounds(material, objects, new Set([2])).minX).toBe(0);
  });

  it('centres a selection on the material (F9)', () => {
    expect(centreOnMaterial([10, 10, 30, 20], material)).toEqual({ dx: 30, dy: 10 });
  });
});
