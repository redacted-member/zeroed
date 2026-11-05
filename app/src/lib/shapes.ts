// Builders for the shapes the Create Vectors forms make. Every builder
// returns Path JSON in job millimetres; the kernel stores it unchanged.

import type { Path, Point, Segment } from '../bridge/document';
import { arcFromBulge, polylinePath } from './geometry';

export type Anchor =
  | 'centre'
  | 'bottomLeft'
  | 'bottomRight'
  | 'topLeft'
  | 'topRight'
  | 'left'
  | 'right'
  | 'top'
  | 'bottom';

/** Bottom left corner of a width x height box placed by its anchor point. */
export function boxOrigin(
  anchor: Anchor,
  x: number,
  y: number,
  width: number,
  height: number,
): Point {
  const dx =
    anchor === 'centre' || anchor === 'top' || anchor === 'bottom'
      ? width / 2
      : anchor === 'bottomRight' || anchor === 'topRight' || anchor === 'right'
        ? width
        : 0;
  const dy =
    anchor === 'centre' || anchor === 'left' || anchor === 'right'
      ? height / 2
      : anchor === 'topLeft' || anchor === 'topRight' || anchor === 'top'
        ? height
        : 0;
  return { x: x - dx, y: y - dy };
}

export type CornerType = 'none' | 'external' | 'internal';

/**
 * Rectangle with optional rounded corners. External corners bulge outward
 * (fillets); internal corners are scooped inward like a picture frame.
 */
export function rectangle(
  x: number,
  y: number,
  width: number,
  height: number,
  corner: CornerType = 'none',
  radius = 0,
): Path {
  const r = Math.max(0, Math.min(radius, width / 2, height / 2));
  if (corner === 'none' || r <= 0) {
    return polylinePath(
      [
        { x, y },
        { x: x + width, y },
        { x: x + width, y: y + height },
        { x, y: y + height },
      ],
      true,
    );
  }
  // Quarter arcs: external corners sweep 90 degrees CCW (bulge tan(pi/8)),
  // internal ones sweep the other way.
  const bulge = corner === 'external' ? Math.tan(Math.PI / 8) : -Math.tan(Math.PI / 8);
  const start: Point = { x: x + r, y };
  const segments: Segment[] = [
    { kind: 'line', to: { x: x + width - r, y } },
    { kind: 'arc', to: { x: x + width, y: y + r }, bulge },
    { kind: 'line', to: { x: x + width, y: y + height - r } },
    { kind: 'arc', to: { x: x + width - r, y: y + height }, bulge },
    { kind: 'line', to: { x: x + r, y: y + height } },
    { kind: 'arc', to: { x, y: y + height - r }, bulge },
    { kind: 'line', to: { x, y: y + r } },
    { kind: 'arc', to: { x: x + r, y }, bulge },
  ];
  return { subpaths: [{ start, closed: true, segments }] };
}

/** Circle as two counter clockwise semicircular arcs. */
export function circle(cx: number, cy: number, radius: number): Path {
  return {
    subpaths: [
      {
        start: { x: cx + radius, y: cy },
        closed: true,
        segments: [
          { kind: 'arc', to: { x: cx - radius, y: cy }, bulge: 1 },
          { kind: 'arc', to: { x: cx + radius, y: cy }, bulge: 1 },
        ],
      },
    ],
  };
}

/** Ellipse as four cubic beziers (kappa approximation). */
export function ellipse(cx: number, cy: number, width: number, height: number): Path {
  const a = width / 2;
  const b = height / 2;
  const k = 0.5522847498;
  const start: Point = { x: cx + a, y: cy };
  const segments: Segment[] = [
    {
      kind: 'cubic',
      c1: { x: cx + a, y: cy + b * k },
      c2: { x: cx + a * k, y: cy + b },
      to: { x: cx, y: cy + b },
    },
    {
      kind: 'cubic',
      c1: { x: cx - a * k, y: cy + b },
      c2: { x: cx - a, y: cy + b * k },
      to: { x: cx - a, y: cy },
    },
    {
      kind: 'cubic',
      c1: { x: cx - a, y: cy - b * k },
      c2: { x: cx - a * k, y: cy - b },
      to: { x: cx, y: cy - b },
    },
    {
      kind: 'cubic',
      c1: { x: cx + a * k, y: cy - b },
      c2: { x: cx + a, y: cy - b * k },
      to: start,
    },
  ];
  return { subpaths: [{ start, closed: true, segments }] };
}

/** Regular polygon; the radius reaches the vertices. */
export function polygon(
  cx: number,
  cy: number,
  sides: number,
  radius: number,
  startAngleDeg = 90,
): Path {
  const n = Math.max(3, Math.round(sides));
  const points: Point[] = [];
  for (let i = 0; i < n; i += 1) {
    const angle = (startAngleDeg * Math.PI) / 180 + (i * 2 * Math.PI) / n;
    points.push({ x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) });
  }
  return polylinePath(points, true);
}

/** Star with alternating outer and inner vertices, first point straight up. */
export function star(cx: number, cy: number, points: number, outer: number, inner: number): Path {
  const n = Math.max(3, Math.round(points));
  const vertices: Point[] = [];
  for (let i = 0; i < n * 2; i += 1) {
    const angle = Math.PI / 2 + (i * Math.PI) / n;
    const radius = i % 2 === 0 ? outer : inner;
    vertices.push({ x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) });
  }
  return polylinePath(vertices, true);
}

/** Open arc through three points: start, a point on the arc, end. */
export function arcThreePoints(start: Point, through: Point, end: Point): Path | null {
  const ax = start.x;
  const ay = start.y;
  const bx = through.x;
  const by = through.y;
  const cx = end.x;
  const cy = end.y;
  const d = 2 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
  if (Math.abs(d) < 1e-12) {
    return null;
  }
  const a2 = ax * ax + ay * ay;
  const b2 = bx * bx + by * by;
  const c2 = cx * cx + cy * cy;
  const ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
  const uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
  const startAngle = Math.atan2(ay - uy, ax - ux);
  const throughAngle = Math.atan2(by - uy, bx - ux);
  const endAngle = Math.atan2(cy - uy, cx - ux);
  const ccw = (from: number, to: number) =>
    (((to - from) % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
  // Choose the direction whose sweep passes through the middle point.
  const sweepCcw = ccw(startAngle, endAngle);
  const throughCcw = ccw(startAngle, throughAngle);
  const sweep = throughCcw <= sweepCcw ? sweepCcw : sweepCcw - 2 * Math.PI;
  return {
    subpaths: [
      {
        start: { ...start },
        closed: false,
        segments: [{ kind: 'arc', to: { ...end }, bulge: Math.tan(sweep / 4) }],
      },
    ],
  };
}

/** Open arc from a centre, a start point and an end direction, counter clockwise unless `clockwise`. */
export function arcCentreStartEnd(
  centre: Point,
  start: Point,
  end: Point,
  clockwise = false,
): Path {
  const radius = Math.hypot(start.x - centre.x, start.y - centre.y);
  const startAngle = Math.atan2(start.y - centre.y, start.x - centre.x);
  const endAngle = Math.atan2(end.y - centre.y, end.x - centre.x);
  let sweep = endAngle - startAngle;
  if (clockwise) {
    while (sweep > 0) {
      sweep -= 2 * Math.PI;
    }
  } else {
    while (sweep < 0) {
      sweep += 2 * Math.PI;
    }
  }
  const to: Point = {
    x: centre.x + radius * Math.cos(endAngle),
    y: centre.y + radius * Math.sin(endAngle),
  };
  return {
    subpaths: [
      {
        start: { ...start },
        closed: false,
        segments: [{ kind: 'arc', to, bulge: Math.tan(sweep / 4) }],
      },
    ],
  };
}

/** Radius of an arc segment, for form fields that show it. */
export function arcRadius(from: Point, segment: Segment): number | null {
  if (segment.kind !== 'arc') {
    return null;
  }
  return arcFromBulge(from, segment.to, segment.bulge)?.radius ?? null;
}

/** Point at a distance and angle (degrees, counter clockwise from +X) from another point. */
export function polar(from: Point, length: number, angleDeg: number): Point {
  const angle = (angleDeg * Math.PI) / 180;
  return { x: from.x + length * Math.cos(angle), y: from.y + length * Math.sin(angle) };
}

/** Snaps an angle to the nearest multiple of `stepDeg`. */
export function snapAngle(angleDeg: number, stepDeg = 15): number {
  return Math.round(angleDeg / stepDeg) * stepDeg;
}
