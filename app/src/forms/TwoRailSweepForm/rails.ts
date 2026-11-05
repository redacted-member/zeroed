// Rails of a Two-Rail Sweep as the kernel reads them: flattened, started at
// the chosen point of a closed rail, reversed when asked, and measured by
// length, so positions along them can be turned into the fractions the
// kernel takes and drawn in the view.

import type { Path, Point } from '../../bridge/document';
import { flattenSubPath, polylinePath } from '../../lib/geometry';

/** Chord tolerance for rails, mm. */
const FLATTEN = 0.02;

export interface Rail {
  /** From the start to the end; a closed rail ends where it starts. */
  points: Point[];
  closed: boolean;
  /** Distance along the rail at each point. */
  lengths: number[];
  /** Fractions of the length at the path's own nodes, in order. */
  nodes: number[];
}

function distance(a: Point, b: Point): number {
  return Math.hypot(b.x - a.x, b.y - a.y);
}

function lerp(a: Point, b: Point, t: number): Point {
  return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
}

/** Nearest point on segment ab to p, with its place along it (0 to 1). */
function nearestOn(a: Point, b: Point, p: Point): { point: Point; t: number } {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2));
  return { point: lerp(a, b, t), t };
}

/** A closed loop (last point not repeated) started at its point nearest
 * `from`, inserting that point when it falls inside a span. */
export function startAt(points: readonly Point[], from: Point): Point[] {
  const n = points.length;
  let best = { d: Infinity, k: 0, point: points[0] ?? from };
  for (let k = 0; k < n; k += 1) {
    const a = points[k] ?? from;
    const b = points[(k + 1) % n] ?? from;
    const { point } = nearestOn(a, b, from);
    const d = distance(point, from);
    if (d < best.d) {
      best = { d, k, point };
    }
  }
  const out: Point[] = [best.point];
  for (let s = 1; s <= n; s += 1) {
    const q = points[(best.k + s) % n] ?? from;
    const last = out[out.length - 1] ?? best.point;
    if (distance(q, best.point) > 1e-9 && distance(last, q) > 1e-9) {
      out.push(q);
    }
  }
  return out;
}

function measure(points: readonly Point[]): number[] {
  const lengths = [0];
  let total = 0;
  for (let i = 1; i < points.length; i += 1) {
    total += distance(points[i - 1] ?? { x: 0, y: 0 }, points[i] ?? { x: 0, y: 0 });
    lengths.push(total);
  }
  return lengths;
}

/** The rail of a single subpath vector, or null for anything else. */
export function railOf(path: Path, reverse: boolean, start: Point | null): Rail | null {
  const [sub] = path.subpaths;
  if (sub === undefined || path.subpaths.length !== 1) {
    return null;
  }
  const flat = flattenSubPath(sub, FLATTEN).points;
  let points: Point[] = [];
  for (let i = 0; i + 1 < flat.length; i += 2) {
    points.push({ x: flat[i] ?? 0, y: flat[i + 1] ?? 0 });
  }
  const closed = sub.closed;
  if (closed) {
    const first = points[0];
    const last = points[points.length - 1];
    if (first !== undefined && last !== undefined && distance(first, last) < 1e-9) {
      points.pop();
    }
    if (start !== null) {
      points = startAt(points, start);
    }
    const head = points[0];
    if (head !== undefined) {
      points.push(head);
    }
  }
  if (reverse) {
    points.reverse();
  }
  const lengths = measure(points);
  const total = lengths[lengths.length - 1] ?? 0;
  if (points.length < 2 || total <= 0) {
    return null;
  }
  const rail: Rail = { points, closed, lengths, nodes: [] };
  const corners = [sub.start, ...sub.segments.map((s) => s.to)];
  const nodes = corners.map((p) => nearestOnRail(rail, p).f);
  if (!closed) {
    rail.nodes = nodes.sort((a, b) => a - b);
  } else {
    rail.nodes = [...new Set(nodes.map((f) => (f > 1 - 1e-9 ? 0 : f)))].sort((a, b) => a - b);
  }
  return rail;
}

export function railLength(rail: Rail): number {
  return rail.lengths[rail.lengths.length - 1] ?? 0;
}

/** The point `f` of the way along (clamped, or wrapped when closed). */
export function pointAt(rail: Rail, f: number): Point {
  const g = rail.closed ? ((f % 1) + 1) % 1 : Math.max(0, Math.min(1, f));
  const target = g * railLength(rail);
  let k = 1;
  while (k < rail.lengths.length - 1 && (rail.lengths[k] ?? 0) < target) {
    k += 1;
  }
  const l0 = rail.lengths[k - 1] ?? 0;
  const l1 = rail.lengths[k] ?? 0;
  const a = rail.points[k - 1] ?? { x: 0, y: 0 };
  const b = rail.points[k] ?? a;
  return lerp(a, b, l1 > l0 ? (target - l0) / (l1 - l0) : 0);
}

/** The nearest place on the rail to `p`: fraction, point and distance. */
export function nearestOnRail(rail: Rail, p: Point): { f: number; point: Point; distance: number } {
  let best = { f: 0, point: rail.points[0] ?? p, distance: Infinity };
  const total = railLength(rail);
  for (let k = 1; k < rail.points.length; k += 1) {
    const a = rail.points[k - 1] ?? p;
    const b = rail.points[k] ?? p;
    const { point, t } = nearestOn(a, b, p);
    const d = distance(point, p);
    if (d < best.distance) {
      const l0 = rail.lengths[k - 1] ?? 0;
      const l1 = rail.lengths[k] ?? 0;
      best = { f: total > 0 ? (l0 + (l1 - l0) * t) / total : 0, point, distance: d };
    }
  }
  return best;
}

/** The node of the rail nearest `p`, as a point. */
export function nearestNode(rail: Rail, p: Point): Point {
  let best = rail.points[0] ?? p;
  let bestD = Infinity;
  for (const f of rail.nodes) {
    const q = pointAt(rail, f);
    const d = distance(q, p);
    if (d < bestD) {
      bestD = d;
      best = q;
    }
  }
  return best;
}

/** A small square around `p`, `size` mm across. */
export function markerPath(p: Point, size: number): Path {
  const h = size / 2;
  return polylinePath(
    [
      { x: p.x - h, y: p.y - h },
      { x: p.x + h, y: p.y - h },
      { x: p.x + h, y: p.y + h },
      { x: p.x - h, y: p.y + h },
    ],
    true,
  );
}

/** Chevrons along the rail pointing the way it runs, `count` of them. */
export function arrowPaths(rail: Rail, count: number, size: number): Path[] {
  const out: Path[] = [];
  for (let k = 1; k <= count; k += 1) {
    const f = k / (count + 1);
    const at = pointAt(rail, f);
    const ahead = pointAt(rail, Math.min(1, f + 0.002));
    const behind = pointAt(rail, Math.max(0, f - 0.002));
    const dx = ahead.x - behind.x;
    const dy = ahead.y - behind.y;
    const len = Math.hypot(dx, dy);
    if (len < 1e-9) {
      continue;
    }
    const ux = dx / len;
    const uy = dy / len;
    const back = { x: at.x - ux * size, y: at.y - uy * size };
    out.push(
      polylinePath(
        [
          { x: back.x - uy * size * 0.6, y: back.y + ux * size * 0.6 },
          at,
          { x: back.x + uy * size * 0.6, y: back.y - ux * size * 0.6 },
        ],
        false,
      ),
    );
  }
  return out;
}
