// Pure geometry for Extend Vectors: the straight extension off an open end,
// where it meets a span or another end's extension, and the preview paths.

import type { Path, Point } from '../../bridge/document';
import { flattenSubPath, flatIntersections, type FlatSubPath } from '../../lib/geometry';

/** An open end of a vector as the kernel names it (ops.extendTo). */
export interface OpenEndRef {
  id: number;
  subpath: number;
  atEnd: boolean;
}

/** A straight extension: from the end point along a unit direction. */
export interface Ray {
  origin: Point;
  dir: Point;
}

/** Flattening tolerance used for tangents and hits, in millimetres. */
const FLAT = 0.01;

/**
 * The extension off one end of an open subpath: it starts at the end node
 * and carries on the direction the vector arrives there with. Null when
 * the subpath is closed or has no length.
 */
export function endRay(path: Path, subpath: number, atEnd: boolean): Ray | null {
  const sub = path.subpaths[subpath];
  if (sub === undefined || sub.closed || sub.segments.length === 0) {
    return null;
  }
  const flat = flattenSubPath(sub, FLAT).points;
  const count = flat.length / 2;
  const at = (i: number): Point => ({ x: flat[2 * i] ?? 0, y: flat[2 * i + 1] ?? 0 });
  const end = atEnd ? at(count - 1) : at(0);
  // The nearest earlier point that is not on top of the end.
  for (let k = 1; k < count; k += 1) {
    const before = atEnd ? at(count - 1 - k) : at(k);
    const dx = end.x - before.x;
    const dy = end.y - before.y;
    const length = Math.hypot(dx, dy);
    if (length > 1e-9) {
      return { origin: end, dir: { x: dx / length, y: dy / length } };
    }
  }
  return null;
}

/** The point `distance` along a ray. */
export function along(ray: Ray, distance: number): Point {
  return { x: ray.origin.x + ray.dir.x * distance, y: ray.origin.y + ray.dir.y * distance };
}

/** Where two extensions meet ahead of both ends, or null. */
export function raysMeet(a: Ray, b: Ray): Point | null {
  const cross = a.dir.x * b.dir.y - a.dir.y * b.dir.x;
  if (Math.abs(cross) < 1e-12) {
    return null;
  }
  const qx = b.origin.x - a.origin.x;
  const qy = b.origin.y - a.origin.y;
  const t = (qx * b.dir.y - qy * b.dir.x) / cross;
  const u = (qx * a.dir.y - qy * a.dir.x) / cross;
  return t > 1e-9 && u > 1e-9 ? along(a, t) : null;
}

/** The points where the extension (up to `length`) crosses `path`, nearest
 * the end first; the end itself does not count. */
export function rayHits(ray: Ray, length: number, path: Path): Point[] {
  const tip = along(ray, length);
  const line: FlatSubPath[] = [
    { points: [ray.origin.x, ray.origin.y, tip.x, tip.y], closed: false },
  ];
  const flat = path.subpaths.map((sub) => flattenSubPath(sub, FLAT));
  const box: [number, number, number, number] = [
    Math.min(ray.origin.x, tip.x) - 1,
    Math.min(ray.origin.y, tip.y) - 1,
    Math.max(ray.origin.x, tip.x) + 1,
    Math.max(ray.origin.y, tip.y) + 1,
  ];
  const distance = (p: Point) => Math.hypot(p.x - ray.origin.x, p.y - ray.origin.y);
  return flatIntersections(line, flat, box)
    .filter((p) => distance(p) > 1e-6)
    .sort((p, q) => distance(p) - distance(q));
}

/** The point of `points` nearest `to`, or null for none. */
export function nearestPoint(points: readonly Point[], to: Point): Point | null {
  let best: Point | null = null;
  let bestD = Infinity;
  for (const p of points) {
    const d = Math.hypot(p.x - to.x, p.y - to.y);
    if (d < bestD) {
      best = p;
      bestD = d;
    }
  }
  return best;
}

/** A straight line as a preview path. */
export function linePath(from: Point, to: Point): Path {
  return { subpaths: [{ start: from, closed: false, segments: [{ kind: 'line', to }] }] };
}

/** A small cross marking a point, `size` millimetres across. */
export function markPath(at: Point, size: number): Path {
  const h = size / 2;
  return {
    subpaths: [
      {
        start: { x: at.x - h, y: at.y - h },
        closed: false,
        segments: [{ kind: 'line', to: { x: at.x + h, y: at.y + h } }],
      },
      {
        start: { x: at.x - h, y: at.y + h },
        closed: false,
        segments: [{ kind: 'line', to: { x: at.x + h, y: at.y - h } }],
      },
    ],
  };
}

/** Whether two open end references name the same end. */
export function sameEnd(a: OpenEndRef, b: OpenEndRef): boolean {
  return a.id === b.id && a.subpath === b.subpath && a.atEnd === b.atEnd;
}
