// Smooth spans through clicked points, shared by Draw Line (dragged curve
// spans) and Draw Curve (a curve through every point), and carrying an open
// vector on from one of its ends.

import type { Path, Point, Segment, SubPath } from '../bridge/document';
import { arcFromBulge } from './geometry';

const TINY = 1e-6;

function unit(v: Point): Point | null {
  const length = Math.hypot(v.x, v.y);
  return length > TINY ? { x: v.x / length, y: v.y / length } : null;
}

/**
 * The segments through `points`: straight where `curves[i]` is false, else
 * a cubic whose ends leave along each point's tangent (a dragged direction,
 * or the direction between the neighbouring points), a third of the span
 * long.
 */
export function spanSegments(
  points: readonly Point[],
  curves: readonly boolean[],
  tangents: ReadonlyArray<Point | null>,
  closed: boolean,
  closingCurve = false,
): SubPath | null {
  const [start] = points;
  if (start === undefined) {
    return null;
  }
  const n = points.length;
  const at = (i: number): Point => points[((i % n) + n) % n] ?? start;
  const tangent = (i: number): Point => {
    const given = tangents[i] ?? null;
    if (given !== null) {
      return given;
    }
    const hasBefore = closed || i > 0;
    const hasAfter = closed || i < n - 1;
    const before = hasBefore ? at(i - 1) : at(i);
    const after = hasAfter ? at(i + 1) : at(i);
    return unit({ x: after.x - before.x, y: after.y - before.y }) ?? { x: 1, y: 0 };
  };
  const segments: Segment[] = [];
  const span = (i: number, curve: boolean) => {
    const a = at(i);
    const b = at(i + 1);
    if (!curve) {
      segments.push({ kind: 'line', to: b });
      return;
    }
    const third = Math.hypot(b.x - a.x, b.y - a.y) / 3;
    const ta = tangent(i);
    const tb = tangent((i + 1) % n);
    segments.push({
      kind: 'cubic',
      c1: { x: a.x + ta.x * third, y: a.y + ta.y * third },
      c2: { x: b.x - tb.x * third, y: b.y - tb.y * third },
      to: b,
    });
  };
  for (let i = 0; i < n - 1; i += 1) {
    span(i, curves[i] ?? false);
  }
  if (closed) {
    span(n - 1, closingCurve);
  }
  return { start, segments, closed };
}

/** The same segments run backwards, ending at `start`. */
export function reverseSegments(start: Point, segments: readonly Segment[]): SubPath {
  const ends = [start, ...segments.map((s) => s.to)];
  const out: Segment[] = [];
  for (let i = segments.length - 1; i >= 0; i -= 1) {
    const segment = segments[i];
    const to = ends[i];
    if (segment === undefined || to === undefined) {
      continue;
    }
    switch (segment.kind) {
      case 'line':
        out.push({ kind: 'line', to });
        break;
      case 'arc':
        out.push({ kind: 'arc', to, bulge: -segment.bulge });
        break;
      case 'cubic':
        out.push({ kind: 'cubic', c1: segment.c2, c2: segment.c1, to });
        break;
    }
  }
  return { start: ends[ends.length - 1] ?? start, segments: out, closed: false };
}

/** Where an open subpath of `path` ends within `tolerance` of `p`. */
export function openEndNear(
  path: Path,
  p: Point,
  tolerance: number,
): { subpath: number; atEnd: boolean; point: Point } | null {
  let best: { subpath: number; atEnd: boolean; point: Point; d: number } | null = null;
  path.subpaths.forEach((sub, subpath) => {
    if (sub.closed) {
      return;
    }
    const end = sub.segments[sub.segments.length - 1]?.to ?? sub.start;
    for (const [point, atEnd] of [
      [sub.start, false],
      [end, true],
    ] as const) {
      const d = Math.hypot(point.x - p.x, point.y - p.y);
      if (d <= tolerance && (best === null || d < best.d)) {
        best = { subpath, atEnd, point, d };
      }
    }
  });
  if (best === null) {
    return null;
  }
  const found: { subpath: number; atEnd: boolean; point: Point } = best;
  return { subpath: found.subpath, atEnd: found.atEnd, point: found.point };
}

/**
 * `path` with the drawn line `drawn` (which starts on the vector's end)
 * joined onto the open subpath ending at that end.
 */
export function extendPath(path: Path, drawn: SubPath, atEnd: boolean): Path | null {
  const found = openEndNear(path, drawn.start, 1e-6);
  if (found === null || found.atEnd !== atEnd) {
    return null;
  }
  const subpaths = path.subpaths.map((sub, index) => {
    if (index !== found.subpath) {
      return sub;
    }
    if (atEnd) {
      return { ...sub, segments: [...sub.segments, ...drawn.segments] };
    }
    const back = reverseSegments(drawn.start, drawn.segments);
    return { start: back.start, segments: [...back.segments, ...sub.segments], closed: false };
  });
  return { subpaths };
}

export interface Circle {
  centre: Point;
  radius: number;
}

/** The circle of the arc span of `path` passing closest to `p`, if any. */
export function nearestArc(path: Path, p: Point): Circle | null {
  let best: (Circle & { d: number }) | null = null;
  for (const sub of path.subpaths) {
    let from = sub.start;
    for (const segment of sub.segments) {
      if (segment.kind === 'arc') {
        const arc = arcFromBulge(from, segment.to, segment.bulge);
        if (arc !== null) {
          const d = Math.abs(Math.hypot(p.x - arc.centre.x, p.y - arc.centre.y) - arc.radius);
          if (best === null || d < best.d) {
            best = { centre: arc.centre, radius: arc.radius, d };
          }
        }
      }
      from = segment.to;
    }
  }
  return best === null ? null : { centre: best.centre, radius: best.radius };
}
