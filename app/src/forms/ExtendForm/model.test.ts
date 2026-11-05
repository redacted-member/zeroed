import { describe, expect, it } from 'vitest';

import type { Path } from '../../bridge/document';
import { endRay, nearestPoint, raysMeet, rayHits, sameEnd } from './model';

const line = (x0: number, y0: number, x1: number, y1: number): Path => ({
  subpaths: [
    { start: { x: x0, y: y0 }, closed: false, segments: [{ kind: 'line', to: { x: x1, y: y1 } }] },
  ],
});

describe('extend vectors model', () => {
  it('carries each end on in the direction the vector arrives', () => {
    const path = line(0, 0, 10, 0);
    expect(endRay(path, 0, true)).toEqual({ origin: { x: 10, y: 0 }, dir: { x: 1, y: 0 } });
    expect(endRay(path, 0, false)).toEqual({ origin: { x: 0, y: 0 }, dir: { x: -1, y: 0 } });
    const closed: Path = { subpaths: [{ ...line(0, 0, 1, 1).subpaths[0], closed: true }] };
    expect(endRay(closed, 0, true)).toBeNull();
  });

  it('finds where two extensions meet ahead of both ends', () => {
    const a = endRay(line(0, 0, 10, 0), 0, true);
    const b = endRay(line(20, 10, 20, 5), 0, true);
    expect(a && b ? raysMeet(a, b) : null).toEqual({ x: 20, y: 0 });
    const behind = endRay(line(20, -5, 20, -10), 0, true);
    expect(a && behind ? raysMeet(a, behind) : null).toBeNull();
  });

  it('finds the spans the extension crosses, nearest first', () => {
    const ray = endRay(line(0, 0, 10, 0), 0, true);
    const target: Path = {
      subpaths: [...line(30, -5, 30, 5).subpaths, ...line(20, -5, 20, 5).subpaths],
    };
    const hits = ray ? rayHits(ray, 100, target) : [];
    expect(hits.map((p) => Math.round(p.x))).toEqual([20, 30]);
    expect(nearestPoint(hits, { x: 29, y: 1 })?.x).toBeCloseTo(30);
  });

  it('tells ends apart', () => {
    expect(sameEnd({ id: 1, subpath: 0, atEnd: true }, { id: 1, subpath: 0, atEnd: true })).toBe(
      true,
    );
    expect(sameEnd({ id: 1, subpath: 0, atEnd: true }, { id: 1, subpath: 0, atEnd: false })).toBe(
      false,
    );
  });
});
