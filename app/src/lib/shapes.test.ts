import { describe, expect, it } from 'vitest';

import { bboxOfFlat, flattenPath, pathNodes } from './geometry';
import {
  arcCentreStartEnd,
  arcThreePoints,
  boxOrigin,
  circle,
  ellipse,
  polar,
  polygon,
  rectangle,
  snapAngle,
  star,
} from './shapes';

const round = (v: number) => Math.round(v * 1000) / 1000;
const coarse = (v: number) => Math.round(v * 100) / 100;

describe('boxOrigin', () => {
  it('places the box by each anchor', () => {
    expect(boxOrigin('bottomLeft', 10, 10, 4, 2)).toEqual({ x: 10, y: 10 });
    expect(boxOrigin('centre', 10, 10, 4, 2)).toEqual({ x: 8, y: 9 });
    expect(boxOrigin('topRight', 10, 10, 4, 2)).toEqual({ x: 6, y: 8 });
    expect(boxOrigin('top', 10, 10, 4, 2)).toEqual({ x: 8, y: 8 });
  });
});

describe('rectangle', () => {
  it('is four lines without corners', () => {
    const path = rectangle(0, 0, 10, 5);
    expect(path.subpaths[0]?.segments.every((s) => s.kind === 'line')).toBe(true);
    expect(bboxOfFlat(flattenPath(path, 0.01))).toEqual([0, 0, 10, 5]);
  });

  it('adds four arcs for external corners and keeps the bounds', () => {
    const path = rectangle(0, 0, 10, 5, 'external', 1);
    const kinds = path.subpaths[0]?.segments.map((s) => s.kind);
    expect(kinds?.filter((k) => k === 'arc')).toHaveLength(4);
    const bbox = bboxOfFlat(flattenPath(path, 0.001)).map(round);
    expect(bbox).toEqual([0, 0, 10, 5]);
  });

  it('scoops the corners inward for internal corners', () => {
    const path = rectangle(0, 0, 10, 5, 'internal', 1);
    const flat = flattenPath(path, 0.001);
    // The corner region near (0, 0) must be cut away: no point at the corner itself.
    const points = flat[0]?.points ?? [];
    let nearest = Infinity;
    for (let i = 0; i < points.length; i += 2) {
      nearest = Math.min(nearest, Math.hypot(points[i] ?? 0, points[i + 1] ?? 0));
    }
    expect(nearest).toBeGreaterThan(0.9);
  });

  it('clamps the radius to half the smaller side', () => {
    const path = rectangle(0, 0, 10, 2, 'external', 5);
    const bbox = bboxOfFlat(flattenPath(path, 0.001)).map(round);
    expect(bbox).toEqual([0, 0, 10, 2]);
  });
});

describe('round shapes', () => {
  it('circle flattens to the right bounds', () => {
    expect(bboxOfFlat(flattenPath(circle(5, 5, 2), 0.001)).map(round)).toEqual([3, 3, 7, 7]);
  });

  it('ellipse flattens to the right bounds', () => {
    const bbox = bboxOfFlat(flattenPath(ellipse(0, 0, 20, 10), 0.001)).map(round);
    expect(bbox).toEqual([-10, -5, 10, 5]);
  });
});

describe('polygon and star', () => {
  it('builds n vertices with the vertex radius', () => {
    const path = polygon(0, 0, 6, 10);
    const nodes = pathNodes(path);
    expect(nodes).toHaveLength(6);
    expect(nodes.every((n) => Math.abs(Math.hypot(n.x, n.y) - 10) < 1e-9)).toBe(true);
  });

  it('turns the first vertex by the start angle', () => {
    const path = polygon(0, 0, 4, 10, 0);
    const bbox = bboxOfFlat(flattenPath(path, 0.001)).map(round);
    expect(bbox).toEqual([-10, -10, 10, 10]);
  });

  it('star alternates outer and inner radii', () => {
    const nodes = pathNodes(star(0, 0, 5, 10, 4));
    expect(nodes).toHaveLength(10);
    expect(round(Math.hypot(nodes[0]?.x ?? 0, nodes[0]?.y ?? 0))).toBe(10);
    expect(round(Math.hypot(nodes[1]?.x ?? 0, nodes[1]?.y ?? 0))).toBe(4);
  });
});

describe('arcs', () => {
  it('passes through the middle point', () => {
    const path = arcThreePoints({ x: -5, y: 0 }, { x: 0, y: 5 }, { x: 5, y: 0 });
    expect(path).not.toBeNull();
    const flat = flattenPath(path ?? { subpaths: [] }, 0.001);
    const bbox = bboxOfFlat(flat).map(coarse);
    expect(bbox).toEqual([-5, 0, 5, 5]);
  });

  it('flips direction when the middle point is below', () => {
    const path = arcThreePoints({ x: -5, y: 0 }, { x: 0, y: -5 }, { x: 5, y: 0 });
    const bbox = bboxOfFlat(flattenPath(path ?? { subpaths: [] }, 0.001)).map(coarse);
    expect(bbox).toEqual([-5, -5, 5, 0]);
  });

  it('returns null for collinear points', () => {
    expect(arcThreePoints({ x: 0, y: 0 }, { x: 1, y: 1 }, { x: 2, y: 2 })).toBeNull();
  });

  it('builds a quarter arc from centre, start and end', () => {
    const path = arcCentreStartEnd({ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 0, y: 10 });
    const segment = path.subpaths[0]?.segments[0];
    expect(segment?.kind).toBe('arc');
    expect(round(segment?.kind === 'arc' ? segment.bulge : 0)).toBe(round(Math.tan(Math.PI / 8)));
    expect(round(segment?.to.x ?? 1)).toBe(0);
    expect(round(segment?.to.y ?? 0)).toBe(10);
  });
});

describe('polar helpers', () => {
  it('computes polar points and snaps angles', () => {
    const p = polar({ x: 1, y: 1 }, 2, 90);
    expect(round(p.x)).toBe(1);
    expect(round(p.y)).toBe(3);
    expect(snapAngle(52)).toBe(45);
    expect(snapAngle(-7)).toBe(-0);
  });
});
