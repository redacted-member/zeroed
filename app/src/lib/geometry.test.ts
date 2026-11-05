import { describe, expect, it } from 'vitest';

import type { Path } from '../bridge/document';
import {
  applyMatrix,
  arcFromBulge,
  bboxOfFlat,
  circlePath,
  containsPoint,
  flatIntersections,
  flattenPath,
  mirrorAbout,
  multiply,
  nearestOnFlat,
  pathNodes,
  rectPath,
  rotationAbout,
  scaleAbout,
  segmentMidpoint,
  starPath,
  transformPath,
  translation,
} from './geometry';

const close = (a: number, b: number, tolerance = 1e-6) => Math.abs(a - b) <= tolerance;

describe('arcFromBulge', () => {
  it('finds the centre and radius of a quarter circle', () => {
    // A CCW quarter arc from (10, 0) to (0, 10) around the origin has bulge tan(pi/8).
    const arc = arcFromBulge({ x: 10, y: 0 }, { x: 0, y: 10 }, Math.tan(Math.PI / 8));
    expect(arc).not.toBeNull();
    expect(close(arc?.centre.x ?? 1, 0)).toBe(true);
    expect(close(arc?.centre.y ?? 1, 0)).toBe(true);
    expect(close(arc?.radius ?? 0, 10)).toBe(true);
    expect(close(arc?.sweep ?? 0, Math.PI / 2)).toBe(true);
  });

  it('puts the centre on the other side for a clockwise arc', () => {
    const arc = arcFromBulge({ x: 10, y: 0 }, { x: 0, y: 10 }, -Math.tan(Math.PI / 8));
    expect(close(arc?.centre.x ?? 0, 10)).toBe(true);
    expect(close(arc?.centre.y ?? 0, 10)).toBe(true);
  });

  it('handles a semicircle (bulge 1) and a straight line (bulge 0)', () => {
    const semi = arcFromBulge({ x: -5, y: 0 }, { x: 5, y: 0 }, 1);
    expect(close(semi?.radius ?? 0, 5)).toBe(true);
    expect(close(semi?.centre.x ?? 1, 0)).toBe(true);
    expect(arcFromBulge({ x: 0, y: 0 }, { x: 1, y: 1 }, 0)).toBeNull();
  });
});

describe('flattenPath', () => {
  it('flattens a circle to points on the circumference within tolerance', () => {
    const flat = flattenPath(circlePath(0, 0, 10), 0.01);
    expect(flat).toHaveLength(1);
    const points = flat[0]?.points ?? [];
    expect(points.length).toBeGreaterThan(40);
    for (let i = 0; i < points.length; i += 2) {
      expect(close(Math.hypot(points[i] ?? 0, points[i + 1] ?? 0), 10, 0.02)).toBe(true);
    }
    expect(flat[0]?.closed).toBe(true);
  });

  it('keeps a rectangle at four corners', () => {
    const flat = flattenPath(rectPath(0, 0, 4, 3), 0.01);
    expect(flat[0]?.points).toEqual([0, 0, 4, 0, 4, 3, 0, 3, 0, 0]);
    expect(bboxOfFlat(flat)).toEqual([0, 0, 4, 3]);
  });
});

describe('transforms', () => {
  it('composes translation after rotation', () => {
    const m = multiply(translation(10, 0), rotationAbout(90, 0, 0));
    const p = applyMatrix(m, { x: 1, y: 0 });
    expect(close(p.x, 10)).toBe(true);
    expect(close(p.y, 1)).toBe(true);
  });

  it('scales about an anchor', () => {
    const p = applyMatrix(scaleAbout(2, 3, 1, 1), { x: 2, y: 2 });
    expect(p).toEqual({ x: 3, y: 4 });
  });

  it('flips arc bulges when mirroring', () => {
    const mirrored = transformPath(circlePath(0, 0, 5), mirrorAbout('h', 0, 0));
    const segment = mirrored.subpaths[0]?.segments[0];
    expect(segment?.kind).toBe('arc');
    expect(segment?.kind === 'arc' ? segment.bulge : 0).toBe(1);
    // The mirrored circle is still a circle of radius 5.
    const flat = flattenPath(mirrored, 0.01);
    expect(bboxOfFlat(flat).map((v) => Math.round(v * 10) / 10)).toEqual([-5, -5, 5, 5]);
  });
});

describe('queries', () => {
  const rect = flattenPath(rectPath(0, 0, 10, 10), 0.01);

  it('finds the nearest point on a path', () => {
    const hit = nearestOnFlat(rect, 5, 12);
    expect(hit.x).toBe(5);
    expect(hit.y).toBe(10);
    expect(hit.dist).toBe(2);
  });

  it('tests containment even odd', () => {
    expect(containsPoint(rect, 5, 5)).toBe(true);
    expect(containsPoint(rect, 15, 5)).toBe(false);
  });

  it('intersects two overlapping rectangles at two points', () => {
    const other = flattenPath(rectPath(5, -5, 10, 10), 0.01);
    const hits = flatIntersections(rect, other, [-100, -100, 100, 100]);
    const rounded = hits
      .map((h) => [Math.round(h.x), Math.round(h.y)])
      .sort((a, b) => (a[0] ?? 0) - (b[0] ?? 0));
    expect(rounded).toEqual([
      [5, 0],
      [10, 5],
    ]);
  });

  it('lists nodes and midpoints', () => {
    const path: Path = rectPath(0, 0, 2, 2);
    expect(pathNodes(path)).toHaveLength(4);
    const sub = path.subpaths[0];
    const first = sub?.segments[0];
    expect(sub && first ? segmentMidpoint(sub.start, first) : null).toEqual({ x: 1, y: 0 });
  });

  it('builds a star with the requested number of vertices', () => {
    const star = starPath(0, 0, 5, 10, 4);
    expect(pathNodes(star)).toHaveLength(10);
  });
});
