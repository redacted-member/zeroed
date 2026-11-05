// Distortion envelope maths, matching the kernel's geom::distort: edge
// curves are uniform Catmull Rom splines through their nodes (a span can be
// a straight line instead, node editing's line and curve spans), and a point
// of the original box lands on the Coons patch of the four edges. Used for
// the Distort form's live preview and by the mock kernel.

import type { Bbox, Path, Point } from '../bridge/document';
import { flattenPath } from './geometry';

/** Envelope edges as ops.distort takes them. Bottom and top run left to
 * right, left and right run bottom to top (straight when absent). */
export interface EnvelopeEdges {
  bottom: Point[];
  top: Point[];
  left?: Point[];
  right?: Point[];
}

export type EdgeName = 'bottom' | 'top' | 'left' | 'right';

/** Which spans of each edge are straight lines, one flag per span in the
 * edge's node order; an edge left out has curve spans only. */
export type EdgeLines = Partial<Record<EdgeName, boolean[]>>;

export interface Envelope extends EnvelopeEdges {
  /** The original box the envelope was drawn round. */
  bbox: Bbox;
  /** Straight spans; every span a curve when absent. */
  lines?: EdgeLines;
}

function lerp(a: Point, b: Point, t: number): Point {
  return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
}

/** `count` nodes spread evenly from `a` to `b`. */
export function straightNodes(a: Point, b: Point, count: number): Point[] {
  const n = Math.max(2, Math.round(count));
  return Array.from({ length: n }, (_unused, k) => lerp(a, b, k / (n - 1)));
}

/** An envelope that changes nothing: `nodes` nodes on the bottom and top
 * edges, and `sideNodes` on each side when the sides are curved. */
export function identityEnvelope(bbox: Bbox, nodes: number, sideNodes?: number): Envelope {
  const [x0, y0, x1, y1] = bbox;
  const envelope: Envelope = {
    bbox,
    bottom: straightNodes({ x: x0, y: y0 }, { x: x1, y: y0 }, nodes),
    top: straightNodes({ x: x0, y: y1 }, { x: x1, y: y1 }, nodes),
  };
  if (sideNodes !== undefined) {
    envelope.left = straightNodes({ x: x0, y: y0 }, { x: x0, y: y1 }, sideNodes);
    envelope.right = straightNodes({ x: x1, y: y0 }, { x: x1, y: y1 }, sideNodes);
  }
  return envelope;
}

function nodeAt(nodes: readonly Point[], k: number): Point {
  const n = nodes.length;
  const first = nodes[0] ?? { x: 0, y: 0 };
  const last = nodes[n - 1] ?? first;
  if (k < 0) {
    const next = nodes[1] ?? first;
    return { x: 2 * first.x - next.x, y: 2 * first.y - next.y };
  }
  if (k >= n) {
    const before = nodes[n - 2] ?? last;
    return { x: 2 * last.x - before.x, y: 2 * last.y - before.y };
  }
  return nodes[k] ?? first;
}

/** The span of `nodes` holding parameter `t`, and the parameter inside it. */
function spanAt(nodes: readonly Point[], t: number): { k: number; f: number } {
  const s = t * (nodes.length - 1);
  const k = Math.max(0, Math.min(nodes.length - 2, Math.floor(s)));
  return { k, f: s - k };
}

/** Point at parameter `t` of the curve through `nodes` (node k of n at
 * k / (n - 1)), the spans `lines` flags straight; outside 0..1 the end
 * spans carry on. */
export function curveAt(nodes: readonly Point[], t: number, lines?: readonly boolean[]): Point {
  if (nodes.length < 2) {
    return nodes[0] ?? { x: 0, y: 0 };
  }
  const { k, f } = spanAt(nodes, t);
  if (lines?.[k] === true) {
    return lerp(nodeAt(nodes, k), nodeAt(nodes, k + 1), f);
  }
  const [p0, p1, p2, p3] = [
    nodeAt(nodes, k - 1),
    nodeAt(nodes, k),
    nodeAt(nodes, k + 1),
    nodeAt(nodes, k + 2),
  ];
  const blend = (a: number, b: number, c: number, d: number) =>
    0.5 *
    (2 * b +
      (-a + c) * f +
      (2 * a - 5 * b + 4 * c - d) * f * f +
      (-a + 3 * b - 3 * c + d) * f * f * f);
  return { x: blend(p0.x, p1.x, p2.x, p3.x), y: blend(p0.y, p1.y, p2.y, p3.y) };
}

function first(nodes: readonly Point[]): Point {
  return nodes[0] ?? { x: 0, y: 0 };
}

function last(nodes: readonly Point[]): Point {
  return nodes[nodes.length - 1] ?? first(nodes);
}

/** A side curve with its ends moved onto the corners. */
function side(nodes: readonly Point[] | undefined, a: Point, b: Point): Point[] {
  if (nodes === undefined || nodes.length < 2) {
    return [a, b];
  }
  return [a, ...nodes.slice(1, -1), b];
}

/** Where the normalised position (u, v) of the original box lands. */
export function mapUv(envelope: Envelope, u: number, v: number): Point {
  const p00 = first(envelope.bottom);
  const p10 = last(envelope.bottom);
  const p01 = first(envelope.top);
  const p11 = last(envelope.top);
  const lines = envelope.lines ?? {};
  const b = curveAt(envelope.bottom, u, lines.bottom);
  const t = curveAt(envelope.top, u, lines.top);
  const l = curveAt(side(envelope.left, p00, p01), v, lines.left);
  const r = curveAt(side(envelope.right, p10, p11), v, lines.right);
  const blend = (
    bb: number,
    tt: number,
    ll: number,
    rr: number,
    c00: number,
    c10: number,
    c01: number,
    c11: number,
  ) =>
    (1 - v) * bb +
    v * tt +
    (1 - u) * ll +
    u * rr -
    ((1 - u) * (1 - v) * c00 + u * (1 - v) * c10 + (1 - u) * v * c01 + u * v * c11);
  return {
    x: blend(b.x, t.x, l.x, r.x, p00.x, p10.x, p01.x, p11.x),
    y: blend(b.y, t.y, l.y, r.y, p00.y, p10.y, p01.y, p11.y),
  };
}

/** Where a point of the original box lands. */
export function mapPoint(envelope: Envelope, p: Point): Point {
  const [x0, y0, x1, y1] = envelope.bbox;
  const w = x1 - x0;
  const h = y1 - y0;
  if (!(w > 0 && h > 0)) {
    return p;
  }
  return mapUv(envelope, (p.x - x0) / w, (p.y - y0) / h);
}

/** Paths flattened within `tolerance`, cut so no piece is longer than a
 * sixty fourth of the box's shorter side, and mapped: a polyline preview
 * of the distorted result. */
export function distortPreview(
  paths: readonly Path[],
  envelope: Envelope,
  tolerance: number,
): Path[] {
  const [x0, y0, x1, y1] = envelope.bbox;
  const step = Math.max(Math.min(x1 - x0, y1 - y0) / 64, tolerance);
  return paths.map((path) => ({
    subpaths: flattenPath(path, tolerance).map((flat) => {
      const pts: Point[] = [];
      for (let i = 0; i + 1 < flat.points.length; i += 2) {
        pts.push({ x: flat.points[i] ?? 0, y: flat.points[i + 1] ?? 0 });
      }
      if (flat.closed && pts.length > 1) {
        pts.push(pts[0] ?? { x: 0, y: 0 });
      }
      const dense: Point[] = pts.length > 0 ? [pts[0] ?? { x: 0, y: 0 }] : [];
      for (let i = 1; i < pts.length; i += 1) {
        const a = pts[i - 1] ?? { x: 0, y: 0 };
        const b = pts[i] ?? a;
        const pieces = Math.min(
          512,
          Math.max(1, Math.ceil(Math.hypot(b.x - a.x, b.y - a.y) / step)),
        );
        for (let k = 1; k <= pieces; k += 1) {
          dense.push(lerp(a, b, k / pieces));
        }
      }
      const mapped = dense.map((p) => mapPoint(envelope, p));
      const start = mapped[0] ?? { x: 0, y: 0 };
      return {
        start,
        segments: mapped.slice(1).map((to) => ({ kind: 'line' as const, to })),
        closed: false,
      };
    }),
  }));
}

/** The bent envelope's outline as a closed polyline path, `perEdge`
 * points to a side. */
export function envelopeOutline(envelope: Envelope, perEdge = 48): Path {
  const pts: Point[] = [];
  for (let k = 0; k < perEdge; k += 1) {
    pts.push(mapUv(envelope, k / perEdge, 0));
  }
  for (let k = 0; k < perEdge; k += 1) {
    pts.push(mapUv(envelope, 1, k / perEdge));
  }
  for (let k = 0; k < perEdge; k += 1) {
    pts.push(mapUv(envelope, 1 - k / perEdge, 1));
  }
  for (let k = 0; k < perEdge; k += 1) {
    pts.push(mapUv(envelope, 0, 1 - k / perEdge));
  }
  const start = pts[0] ?? { x: 0, y: 0 };
  return {
    subpaths: [
      {
        start,
        segments: pts.slice(1).map((to) => ({ kind: 'line' as const, to })),
        closed: true,
      },
    ],
  };
}

type End = 'first' | 'last';

/** Edge ends that share a corner. */
const CORNER_LINKS: ReadonlyArray<readonly [EdgeName, End, EdgeName, End]> = [
  ['bottom', 'first', 'left', 'first'],
  ['bottom', 'last', 'right', 'first'],
  ['top', 'first', 'left', 'last'],
  ['top', 'last', 'right', 'last'],
];

/** Moves one node, keeping shared corners shared: a corner of the bottom
 * or top edge moves the matching end of a curved side, and the other way
 * round. */
export function moveNode(envelope: Envelope, edge: EdgeName, index: number, to: Point): Envelope {
  const next: Envelope = {
    ...envelope,
    bottom: [...envelope.bottom],
    top: [...envelope.top],
    ...(envelope.left === undefined ? {} : { left: [...envelope.left] }),
    ...(envelope.right === undefined ? {} : { right: [...envelope.right] }),
  };
  const list = next[edge];
  if (list === undefined || index < 0 || index >= list.length) {
    return envelope;
  }
  list[index] = to;
  const at: End | null = index === 0 ? 'first' : index === list.length - 1 ? 'last' : null;
  if (at === null) {
    return next;
  }
  for (const [a, aEnd, b, bEnd] of CORNER_LINKS) {
    const other = a === edge && aEnd === at ? { name: b, end: bEnd } : null;
    const reverse = b === edge && bEnd === at ? { name: a, end: aEnd } : null;
    const link = other ?? reverse;
    const target = link === null ? undefined : next[link.name];
    if (link !== null && target !== undefined && target.length > 0) {
      target[link.end === 'first' ? 0 : target.length - 1] = to;
    }
  }
  return next;
}

/** Every node with its edge and index, corners once (bottom and top own
 * them). */
export function envelopeNodes(
  envelope: Envelope,
): Array<{ edge: EdgeName; index: number; point: Point }> {
  const out: Array<{ edge: EdgeName; index: number; point: Point }> = [];
  envelope.bottom.forEach((point, index) => out.push({ edge: 'bottom', index, point }));
  envelope.top.forEach((point, index) => out.push({ edge: 'top', index, point }));
  for (const edge of ['left', 'right'] as const) {
    const nodes = envelope[edge];
    if (nodes !== undefined) {
      nodes.forEach((point, index) => {
        if (index > 0 && index < nodes.length - 1) {
          out.push({ edge, index, point });
        }
      });
    }
  }
  return out;
}

/** Most nodes one envelope edge may have (the kernel's limit). */
export const MAX_EDGE_NODES = 64;

/** The envelope a fresh distortion starts with: the box's four corners
 * joined by four line spans. */
export function cornerEnvelope(bbox: Bbox): Envelope {
  return {
    ...identityEnvelope(bbox, 2, 2),
    lines: { bottom: [true], top: [true], left: [true], right: [true] },
  };
}

/** The line flags of an edge, one per span (false for a curve span). */
function edgeFlags(envelope: Envelope, edge: EdgeName, spans: number): boolean[] {
  const flags = envelope.lines?.[edge] ?? [];
  return Array.from({ length: spans }, (_unused, k) => flags[k] === true);
}

/** The envelope with an edge's line flags replaced; no key for an edge
 * whose spans are all curves. */
function withFlags(envelope: Envelope, edge: EdgeName, flags: boolean[]): Envelope {
  const lines: EdgeLines = { ...(envelope.lines ?? {}) };
  if (flags.some((f) => f)) {
    lines[edge] = flags;
  } else {
    delete lines[edge];
  }
  const next: Envelope = { ...envelope };
  if (Object.keys(lines).length > 0) {
    next.lines = lines;
  } else {
    delete next.lines;
  }
  return next;
}

/** The envelope with its left and right sides as node lists (straight
 * between the corners when it had none), so they can take nodes too. */
export function withSides(envelope: Envelope): Envelope {
  return {
    ...envelope,
    left: side(envelope.left, first(envelope.bottom), first(envelope.top)),
    right: side(envelope.right, last(envelope.bottom), last(envelope.top)),
  };
}

/** The nodes of one edge, a side's ends on the corners. */
function edgeList(envelope: Envelope, edge: EdgeName): Point[] {
  const sided = withSides(envelope);
  return [...(sided[edge] ?? [])];
}

/** A place on an envelope edge: the edge, the span and the parameter along
 * the whole edge (0 to 1). */
export interface EdgeSpot {
  edge: EdgeName;
  span: number;
  t: number;
}

/** The place on the edges nearest `p` within `tolerance` millimetres, or null. */
export function nearestSpot(envelope: Envelope, p: Point, tolerance: number): EdgeSpot | null {
  let best: EdgeSpot | null = null;
  let bestDistance = tolerance;
  const samples = 96;
  for (const edge of ['bottom', 'top', 'left', 'right'] as const) {
    const nodes = edgeList(envelope, edge);
    const lines = envelope.lines?.[edge];
    for (let k = 0; k <= samples; k += 1) {
      const t = k / samples;
      const q = curveAt(nodes, t, lines);
      const d = Math.hypot(q.x - p.x, q.y - p.y);
      if (d <= bestDistance) {
        bestDistance = d;
        best = { edge, span: spanAt(nodes, t).k, t };
      }
    }
  }
  return best;
}

/** The edge within `tolerance` millimetres of `p`, the nearest one, or null. */
export function nearestEdge(envelope: Envelope, p: Point, tolerance: number): EdgeName | null {
  return nearestSpot(envelope, p, tolerance)?.edge ?? null;
}

/**
 * One more node on `edge`. At a place on a line span the node splits it
 * into two line spans there; on an edge of curve spans only (or with no
 * place given) its nodes spread evenly along the curve it follows. Either
 * way the edge keeps its shape.
 */
export function insertEdgeNode(envelope: Envelope, edge: EdgeName, t?: number): Envelope {
  const nodes = edgeList(envelope, edge);
  if (nodes.length >= MAX_EDGE_NODES) {
    return envelope;
  }
  const flags = edgeFlags(envelope, edge, nodes.length - 1);
  const sided = withSides(envelope);
  if (t !== undefined && flags.some((f) => f)) {
    const { k } = spanAt(nodes, t);
    const at = curveAt(nodes, t, flags);
    const next = [...nodes.slice(0, k + 1), at, ...nodes.slice(k + 1)];
    const nextFlags = [...flags.slice(0, k + 1), flags[k] === true, ...flags.slice(k + 1)];
    return withFlags({ ...sided, [edge]: next }, edge, nextFlags);
  }
  const count = nodes.length + 1;
  const spread = Array.from({ length: count }, (_unused, k) => curveAt(nodes, k / (count - 1)));
  // The ends stay exactly on the corners.
  spread[0] = first(nodes);
  spread[count - 1] = last(nodes);
  return withFlags({ ...sided, [edge]: spread }, edge, []);
}

/** The envelope without one node between the corners of `edge`; corners
 * stay. The two spans it joined become one, a line when both were. */
export function deleteEdgeNode(envelope: Envelope, edge: EdgeName, index: number): Envelope {
  const nodes = edgeList(envelope, edge);
  if (index <= 0 || index >= nodes.length - 1) {
    return envelope;
  }
  const flags = edgeFlags(envelope, edge, nodes.length - 1);
  const joined = flags[index - 1] === true && flags[index] === true;
  const nextFlags = [...flags.slice(0, index - 1), joined, ...flags.slice(index + 1)];
  return withFlags(
    { ...withSides(envelope), [edge]: nodes.filter((_node, k) => k !== index) },
    edge,
    nextFlags,
  );
}

/** Node editing's L and C on the envelope: one span made a straight line
 * or a curve. */
export function setSpanLine(
  envelope: Envelope,
  edge: EdgeName,
  span: number,
  line: boolean,
): Envelope {
  const nodes = edgeList(envelope, edge);
  if (span < 0 || span >= nodes.length - 1) {
    return envelope;
  }
  const flags = edgeFlags(envelope, edge, nodes.length - 1);
  if (flags[span] === line) {
    return envelope;
  }
  flags[span] = line;
  return withFlags(withSides(envelope), edge, flags);
}

/** A job point in the frame turned `angle` degrees about the origin. */
export function toEnvelopeFrame(p: Point, angle: number): Point {
  if (angle === 0) {
    return p;
  }
  const r = (-angle * Math.PI) / 180;
  const [c, s] = [Math.cos(r), Math.sin(r)];
  return { x: p.x * c - p.y * s, y: p.x * s + p.y * c };
}

/** A frame point back in job space. */
export function fromEnvelopeFrame(p: Point, angle: number): Point {
  return toEnvelopeFrame(p, -angle);
}

/** An envelope node: its edge and place on it. */
export interface EnvelopeNodeRef {
  edge: EdgeName;
  index: number;
}

/** The node nearest `p` within `tolerance` millimetres, if any. */
export function envelopeNodeAt(
  envelope: Envelope,
  p: Point,
  tolerance: number,
): EnvelopeNodeRef | null {
  let best: EnvelopeNodeRef | null = null;
  let bestDistance = tolerance;
  for (const node of envelopeNodes(envelope)) {
    const d = Math.hypot(node.point.x - p.x, node.point.y - p.y);
    if (d <= bestDistance) {
      bestDistance = d;
      best = { edge: node.edge, index: node.index };
    }
  }
  return best;
}

/**
 * The node editing keys on an envelope, as on a vector: I inserts a node on
 * the edge under the pointer, D (or Delete) deletes the node under it, else
 * the selected one, and L or C makes the span under the pointer a straight
 * line or a curve. Null when the key does nothing here.
 */
export function envelopeKeyEdit(
  envelope: Envelope,
  key: string,
  pointer: Point | null,
  tolerance: number,
  selected: EnvelopeNodeRef | null,
): Envelope | null {
  const k = key.toLowerCase();
  if (k === 'i' || k === 'l' || k === 'c') {
    const spot = pointer === null ? null : nearestSpot(envelope, pointer, tolerance * 2);
    if (spot === null) {
      return null;
    }
    if (k === 'i') {
      return insertEdgeNode(envelope, spot.edge, spot.t);
    }
    const next = setSpanLine(envelope, spot.edge, spot.span, k === 'l');
    return next === envelope ? null : next;
  }
  if (k === 'd' || k === 'delete' || k === 'backspace') {
    const node =
      (pointer === null ? null : envelopeNodeAt(envelope, pointer, tolerance)) ?? selected;
    if (node === null) {
      return null;
    }
    const next = deleteEdgeNode(envelope, node.edge, node.index);
    return next === envelope ? null : next;
  }
  return null;
}
