import { describe, expect, it } from 'vitest';

import {
  recogniseCircle,
  recogniseEllipse,
  recognisePolygon,
  recogniseRectangle,
  recogniseStar,
} from './shapeEdit';
import { circle, ellipse, polygon, rectangle, star } from './shapes';

describe('shape recognition', () => {
  it('recognises a circle drawn by the circle tool', () => {
    const found = recogniseCircle(circle(10, 20, 5));
    expect(found?.radius).toBeCloseTo(5, 9);
    expect(found?.centre.x).toBeCloseTo(10, 9);
    expect(found?.centre.y).toBeCloseTo(20, 9);
  });

  it('refuses arcs of different circles, open loops and lines', () => {
    const half = {
      subpaths: [
        {
          start: { x: 0, y: 0 },
          closed: false,
          segments: [{ kind: 'arc' as const, to: { x: 10, y: 0 }, bulge: 1 }],
        },
      ],
    };
    expect(recogniseCircle(half)).toBeNull();
    const mixed = {
      subpaths: [
        {
          start: { x: 0, y: 0 },
          closed: true,
          segments: [
            { kind: 'arc' as const, to: { x: 10, y: 0 }, bulge: 1 },
            { kind: 'line' as const, to: { x: 0, y: 0 } },
          ],
        },
      ],
    };
    expect(recogniseCircle(mixed)).toBeNull();
  });

  it('recognises the other shape tools and tells them apart', () => {
    expect(recogniseEllipse(ellipse(5, 5, 20, 10))?.box).toEqual([-5, 0, 15, 10]);
    expect(recogniseEllipse(circle(0, 0, 5))).toBeNull();
    expect(recogniseRectangle(rectangle(0, 0, 30, 20))).toEqual({
      box: [0, 0, 30, 20],
      corner: 'none',
      radius: 0,
    });
    const rounded = recogniseRectangle(rectangle(0, 0, 30, 20, 'external', 4));
    expect(rounded?.corner).toBe('external');
    expect(rounded?.radius).toBeCloseTo(4, 9);
    expect(recogniseRectangle(rectangle(0, 0, 30, 20, 'internal', 4))?.corner).toBe('internal');
    const hexagon = recognisePolygon(polygon(1, 2, 6, 10));
    expect(hexagon?.sides).toBe(6);
    expect(hexagon?.radius).toBeCloseTo(10, 9);
    expect(recognisePolygon(star(0, 0, 5, 10, 4))).toBeNull();
    const five = recogniseStar(star(0, 0, 5, 10, 4));
    expect(
      [five?.points, five?.outer, five?.inner].map((v) => Math.round((v ?? 0) * 1e6) / 1e6),
    ).toEqual([5, 10, 4]);
    expect(recogniseStar(polygon(0, 0, 10, 10))).toBeNull();
  });
});
