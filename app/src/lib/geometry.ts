// Plain geometry shared by the views, the tools and the browser only mock:
// flattening, bounds, transforms, distances and intersections on paths made
// of lines, bulge arcs and cubic beziers.

import type { Bbox, Matrix, Path, Point, Segment, SubPath } from '../bridge/document';

export const IDENTITY: Matrix = [1, 0, 0, 1, 0, 0];

export function multiply(a: Matrix, b: Matrix): Matrix {
  // Applies b first, then a.
  return [
    a[0] * b[0] + a[2] * b[1],
    a[1] * b[0] + a[3] * b[1],
    a[0] * b[2] + a[2] * b[3],
    a[1] * b[2] + a[3] * b[3],
    a[0] * b[4] + a[2] * b[5] + a[4],
    a[1] * b[4] + a[3] * b[5] + a[5],
  ];
}

export function translation(dx: number, dy: number): Matrix {
  return [1, 0, 0, 1, dx, dy];
}

export function rotationAbout(angleDeg: number, cx: number, cy: number): Matrix {
  const angle = (angleDeg * Math.PI) / 180;
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  return [cos, sin, -sin, cos, cx - cos * cx + sin * cy, cy - sin * cx - cos * cy];
}

export function scaleAbout(sx: number, sy: number, cx: number, cy: number): Matrix {
  return [sx, 0, 0, sy, cx - sx * cx, cy - sy * cy];
}

/** Mirror across a vertical (axis x = cx) or horizontal (axis y = cy) line. */
export function mirrorAbout(axis: 'h' | 'v', cx: number, cy: number): Matrix {
  return axis === 'h' ? scaleAbout(-1, 1, cx, cy) : scaleAbout(1, -1, cx, cy);
}

export function applyMatrix(m: Matrix, p: Point): Point {
  return { x: m[0] * p.x + m[2] * p.y + m[4], y: m[1] * p.x + m[3] * p.y + m[5] };
}

export function determinant(m: Matrix): number {
  return m[0] * m[3] - m[1] * m[2];
}

/** Transforms a path. Arcs keep their bulge under similarities, mirrored arcs flip. */
export function transformPath(path: Path, m: Matrix): Path {
  const flip = determinant(m) < 0 ? -1 : 1;
  return {
    subpaths: path.subpaths.map((sub) => ({
      start: applyMatrix(m, sub.start),
      closed: sub.closed,
      segments: sub.segments.map((segment): Segment => {
        switch (segment.kind) {
          case 'line':
            return { kind: 'line', to: applyMatrix(m, segment.to) };
          case 'arc':
            return { kind: 'arc', to: applyMatrix(m, segment.to), bulge: segment.bulge * flip };
          case 'cubic':
            return {
              kind: 'cubic',
              c1: applyMatrix(m, segment.c1),
              c2: applyMatrix(m, segment.c2),
              to: applyMatrix(m, segment.to),
            };
        }
      }),
    })),
  };
}

export interface ArcGeometry {
  centre: Point;
  radius: number;
  startAngle: number;
  sweep: number;
}

/** Centre, radius and angles of a bulge arc from p0 to p1. */
export function arcFromBulge(p0: Point, p1: Point, bulge: number): ArcGeometry | null {
  if (Math.abs(bulge) < 1e-12) {
    return null;
  }
  const dx = p1.x - p0.x;
  const dy = p1.y - p0.y;
  const chord = Math.hypot(dx, dy);
  if (chord < 1e-12) {
    return null;
  }
  // Half sweep alpha: the centre sits on the chord's left normal at
  // (chord / 2) / tan(alpha), which changes side by itself for bulges over 1.
  const sweep = 4 * Math.atan(bulge);
  const alpha = sweep / 2;
  const radius = Math.abs(chord / 2 / Math.sin(alpha));
  const midX = (p0.x + p1.x) / 2;
  const midY = (p0.y + p1.y) / 2;
  const distToCentre = chord / 2 / Math.tan(alpha);
  const nx = -dy / chord;
  const ny = dx / chord;
  const centre = { x: midX + nx * distToCentre, y: midY + ny * distToCentre };
  const startAngle = Math.atan2(p0.y - centre.y, p0.x - centre.x);
  return { centre, radius, startAngle, sweep };
}

export function pointOnArc(arc: ArcGeometry, t: number): Point {
  const angle = arc.startAngle + arc.sweep * t;
  return {
    x: arc.centre.x + arc.radius * Math.cos(angle),
    y: arc.centre.y + arc.radius * Math.sin(angle),
  };
}

function pointOnCubic(p0: Point, c1: Point, c2: Point, p1: Point, t: number): Point {
  const mt = 1 - t;
  const a = mt * mt * mt;
  const b = 3 * mt * mt * t;
  const c = 3 * mt * t * t;
  const d = t * t * t;
  return {
    x: a * p0.x + b * c1.x + c * c2.x + d * p1.x,
    y: a * p0.y + b * c1.y + c * c2.y + d * p1.y,
  };
}

export interface FlatSubPath {
  /** Flat x, y pairs. */
  points: number[];
  closed: boolean;
}

/** Flattens a segment, appending points after its start (which is already emitted). */
export function flattenSegment(
  from: Point,
  segment: Segment,
  tolerance: number,
  out: number[],
): void {
  switch (segment.kind) {
    case 'line':
      out.push(segment.to.x, segment.to.y);
      return;
    case 'arc': {
      const arc = arcFromBulge(from, segment.to, segment.bulge);
      if (arc === null) {
        out.push(segment.to.x, segment.to.y);
        return;
      }
      const maxStep = 2 * Math.acos(Math.max(0, 1 - tolerance / Math.max(arc.radius, 1e-9)));
      const steps = Math.max(1, Math.ceil(Math.abs(arc.sweep) / Math.max(maxStep, 1e-3)));
      for (let i = 1; i < steps; i += 1) {
        const p = pointOnArc(arc, i / steps);
        out.push(p.x, p.y);
      }
      out.push(segment.to.x, segment.to.y);
      return;
    }
    case 'cubic': {
      const length =
        Math.hypot(segment.c1.x - from.x, segment.c1.y - from.y) +
        Math.hypot(segment.c2.x - segment.c1.x, segment.c2.y - segment.c1.y) +
        Math.hypot(segment.to.x - segment.c2.x, segment.to.y - segment.c2.y);
      const steps = Math.min(
        96,
        Math.max(4, Math.ceil(Math.sqrt(length / Math.max(tolerance, 1e-4)) * 1.5)),
      );
      for (let i = 1; i < steps; i += 1) {
        const p = pointOnCubic(from, segment.c1, segment.c2, segment.to, i / steps);
        out.push(p.x, p.y);
      }
      out.push(segment.to.x, segment.to.y);
    }
  }
}

export function flattenSubPath(sub: SubPath, tolerance: number): FlatSubPath {
  const points: number[] = [sub.start.x, sub.start.y];
  let from = sub.start;
  for (const segment of sub.segments) {
    flattenSegment(from, segment, tolerance, points);
    from = segment.to;
  }
  return { points, closed: sub.closed };
}

export function flattenPath(path: Path, tolerance: number): FlatSubPath[] {
  return path.subpaths.map((sub) => flattenSubPath(sub, tolerance));
}

export function emptyBbox(): Bbox {
  return [Infinity, Infinity, -Infinity, -Infinity];
}

export function extendBbox(bbox: Bbox, x: number, y: number): void {
  bbox[0] = Math.min(bbox[0], x);
  bbox[1] = Math.min(bbox[1], y);
  bbox[2] = Math.max(bbox[2], x);
  bbox[3] = Math.max(bbox[3], y);
}

export function unionBbox(a: Bbox, b: Bbox): Bbox {
  return [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[2], b[2]), Math.max(a[3], b[3])];
}

export function bboxOfFlat(flat: FlatSubPath[]): Bbox {
  const bbox = emptyBbox();
  for (const sub of flat) {
    for (let i = 0; i < sub.points.length; i += 2) {
      extendBbox(bbox, sub.points[i] ?? 0, sub.points[i + 1] ?? 0);
    }
  }
  return bbox;
}

export function bboxIsEmpty(bbox: Bbox): boolean {
  return !(bbox[0] <= bbox[2] && bbox[1] <= bbox[3]);
}

export function bboxIntersects(a: Bbox, b: Bbox): boolean {
  return a[0] <= b[2] && a[2] >= b[0] && a[1] <= b[3] && a[3] >= b[1];
}

export function bboxContains(outer: Bbox, inner: Bbox): boolean {
  return (
    inner[0] >= outer[0] && inner[2] <= outer[2] && inner[1] >= outer[1] && inner[3] <= outer[3]
  );
}

export function bboxExpand(bbox: Bbox, by: number): Bbox {
  return [bbox[0] - by, bbox[1] - by, bbox[2] + by, bbox[3] + by];
}

/** Nearest point on the line segment ab to p, with its distance. */
export function nearestOnSegment(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  px: number,
  py: number,
): { x: number; y: number; dist: number; t: number } {
  const dx = bx - ax;
  const dy = by - ay;
  const lengthSq = dx * dx + dy * dy;
  const t =
    lengthSq === 0 ? 0 : Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / lengthSq));
  const x = ax + t * dx;
  const y = ay + t * dy;
  return { x, y, dist: Math.hypot(px - x, py - y), t };
}

/** Nearest point on a flattened path to p. */
export function nearestOnFlat(
  flat: FlatSubPath[],
  px: number,
  py: number,
): { x: number; y: number; dist: number } {
  let best = { x: px, y: py, dist: Infinity };
  for (const sub of flat) {
    const n = sub.points.length / 2;
    const segmentCount = sub.closed ? n : n - 1;
    for (let i = 0; i < segmentCount; i += 1) {
      const j = (i + 1) % n;
      const hit = nearestOnSegment(
        sub.points[2 * i] ?? 0,
        sub.points[2 * i + 1] ?? 0,
        sub.points[2 * j] ?? 0,
        sub.points[2 * j + 1] ?? 0,
        px,
        py,
      );
      if (hit.dist < best.dist) {
        best = { x: hit.x, y: hit.y, dist: hit.dist };
      }
    }
  }
  return best;
}

/** Even-odd containment of p in the closed subpaths of a flattened path. */
export function containsPoint(flat: FlatSubPath[], px: number, py: number): boolean {
  let inside = false;
  for (const sub of flat) {
    if (!sub.closed) {
      continue;
    }
    const n = sub.points.length / 2;
    for (let i = 0, j = n - 1; i < n; j = i, i += 1) {
      const xi = sub.points[2 * i] ?? 0;
      const yi = sub.points[2 * i + 1] ?? 0;
      const xj = sub.points[2 * j] ?? 0;
      const yj = sub.points[2 * j + 1] ?? 0;
      if (yi > py !== yj > py && px < ((xj - xi) * (py - yi)) / (yj - yi) + xi) {
        inside = !inside;
      }
    }
  }
  return inside;
}

function segmentIntersection(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  cx: number,
  cy: number,
  dx: number,
  dy: number,
): Point | null {
  const r = { x: bx - ax, y: by - ay };
  const s = { x: dx - cx, y: dy - cy };
  const denominator = r.x * s.y - r.y * s.x;
  if (Math.abs(denominator) < 1e-12) {
    return null;
  }
  const t = ((cx - ax) * s.y - (cy - ay) * s.x) / denominator;
  const u = ((cx - ax) * r.y - (cy - ay) * r.x) / denominator;
  if (t < -1e-9 || t > 1 + 1e-9 || u < -1e-9 || u > 1 + 1e-9) {
    return null;
  }
  return { x: ax + t * r.x, y: ay + t * r.y };
}

/** Intersections between two flattened paths, limited to a search box. */
export function flatIntersections(a: FlatSubPath[], b: FlatSubPath[], within: Bbox): Point[] {
  const found: Point[] = [];
  for (const sa of a) {
    const na = sa.points.length / 2;
    const countA = sa.closed ? na : na - 1;
    for (let i = 0; i < countA; i += 1) {
      const i2 = (i + 1) % na;
      const ax = sa.points[2 * i] ?? 0;
      const ay = sa.points[2 * i + 1] ?? 0;
      const bx = sa.points[2 * i2] ?? 0;
      const by = sa.points[2 * i2 + 1] ?? 0;
      if (
        Math.max(ax, bx) < within[0] ||
        Math.min(ax, bx) > within[2] ||
        Math.max(ay, by) < within[1] ||
        Math.min(ay, by) > within[3]
      ) {
        continue;
      }
      for (const sb of b) {
        const nb = sb.points.length / 2;
        const countB = sb.closed ? nb : nb - 1;
        for (let j = 0; j < countB; j += 1) {
          const j2 = (j + 1) % nb;
          const hit = segmentIntersection(
            ax,
            ay,
            bx,
            by,
            sb.points[2 * j] ?? 0,
            sb.points[2 * j + 1] ?? 0,
            sb.points[2 * j2] ?? 0,
            sb.points[2 * j2 + 1] ?? 0,
          );
          if (
            hit !== null &&
            hit.x >= within[0] &&
            hit.x <= within[2] &&
            hit.y >= within[1] &&
            hit.y <= within[3]
          ) {
            found.push(hit);
          }
        }
      }
    }
  }
  return found;
}

/** Does any segment of the flattened path touch the rectangle. */
export function flatTouchesRect(flat: FlatSubPath[], rect: Bbox): boolean {
  const corners: Array<[number, number, number, number]> = [
    [rect[0], rect[1], rect[2], rect[1]],
    [rect[2], rect[1], rect[2], rect[3]],
    [rect[2], rect[3], rect[0], rect[3]],
    [rect[0], rect[3], rect[0], rect[1]],
  ];
  for (const sub of flat) {
    const n = sub.points.length / 2;
    const count = sub.closed ? n : n - 1;
    for (let i = 0; i < count; i += 1) {
      const j = (i + 1) % n;
      const ax = sub.points[2 * i] ?? 0;
      const ay = sub.points[2 * i + 1] ?? 0;
      const bx = sub.points[2 * j] ?? 0;
      const by = sub.points[2 * j + 1] ?? 0;
      const inside = (x: number, y: number) =>
        x >= rect[0] && x <= rect[2] && y >= rect[1] && y <= rect[3];
      if (inside(ax, ay) || inside(bx, by)) {
        return true;
      }
      for (const [cx, cy, dx, dy] of corners) {
        if (segmentIntersection(ax, ay, bx, by, cx, cy, dx, dy) !== null) {
          return true;
        }
      }
    }
  }
  return false;
}

/** Midpoint of a segment starting at `from`. */
export function segmentMidpoint(from: Point, segment: Segment): Point {
  switch (segment.kind) {
    case 'line':
      return { x: (from.x + segment.to.x) / 2, y: (from.y + segment.to.y) / 2 };
    case 'arc': {
      const arc = arcFromBulge(from, segment.to, segment.bulge);
      return arc === null
        ? { x: (from.x + segment.to.x) / 2, y: (from.y + segment.to.y) / 2 }
        : pointOnArc(arc, 0.5);
    }
    case 'cubic':
      return pointOnCubic(from, segment.c1, segment.c2, segment.to, 0.5);
  }
}

/** Every node (segment start point) of a path. */
export function pathNodes(path: Path): Point[] {
  const nodes: Point[] = [];
  for (const sub of path.subpaths) {
    nodes.push(sub.start);
    const last = sub.segments.length - (sub.closed ? 1 : 0);
    for (let i = 0; i < last; i += 1) {
      const segment = sub.segments[i];
      if (segment !== undefined) {
        nodes.push(segment.to);
      }
    }
  }
  return nodes;
}

// Shape builders used by the mock's sample document and by tests.

export function rectPath(x: number, y: number, width: number, height: number): Path {
  return {
    subpaths: [
      {
        start: { x, y },
        closed: true,
        segments: [
          { kind: 'line', to: { x: x + width, y } },
          { kind: 'line', to: { x: x + width, y: y + height } },
          { kind: 'line', to: { x, y: y + height } },
          { kind: 'line', to: { x, y } },
        ],
      },
    ],
  };
}

export function circlePath(cx: number, cy: number, radius: number): Path {
  return {
    subpaths: [
      {
        start: { x: cx - radius, y: cy },
        closed: true,
        segments: [
          { kind: 'arc', to: { x: cx + radius, y: cy }, bulge: -1 },
          { kind: 'arc', to: { x: cx - radius, y: cy }, bulge: -1 },
        ],
      },
    ],
  };
}

export function starPath(
  cx: number,
  cy: number,
  points: number,
  outer: number,
  inner: number,
): Path {
  const segments: Segment[] = [];
  const vertex = (index: number): Point => {
    const angle = Math.PI / 2 + (index * Math.PI) / points;
    const radius = index % 2 === 0 ? outer : inner;
    return { x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) };
  };
  const start = vertex(0);
  for (let i = 1; i < points * 2; i += 1) {
    segments.push({ kind: 'line', to: vertex(i) });
  }
  segments.push({ kind: 'line', to: start });
  return { subpaths: [{ start, closed: true, segments }] };
}

export function polylinePath(points: Point[], closed = false): Path {
  const [start, ...rest] = points;
  if (start === undefined) {
    return { subpaths: [] };
  }
  const segments: Segment[] = rest.map((to) => ({ kind: 'line', to }));
  if (closed) {
    segments.push({ kind: 'line', to: start });
  }
  return { subpaths: [{ start, closed, segments }] };
}
