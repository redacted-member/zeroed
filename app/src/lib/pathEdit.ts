// Node editing on Path JSON. Every function returns a new path; the caller
// sends it to the kernel with entities.replace.

import type { Path, Point, Segment, SubPath } from '../bridge/document';
import { arcFromBulge, flattenSegment, nearestOnSegment, pointOnArc } from './geometry';

export interface NodeRef {
  subpath: number;
  node: number;
}

export interface SegmentRef {
  subpath: number;
  segment: number;
}

export interface PathNode extends NodeRef {
  x: number;
  y: number;
  /** True when the node joins two cubic spans with collinear handles. */
  smooth: boolean;
  start: boolean;
}

export interface ControlHandle {
  subpath: number;
  segment: number;
  which: 'c1' | 'c2';
  x: number;
  y: number;
  anchorX: number;
  anchorY: number;
}

function clone(path: Path): Path {
  return JSON.parse(JSON.stringify(path)) as Path;
}

/** Start point of a segment. */
export function segmentStart(sub: SubPath, index: number): Point {
  if (index === 0) {
    return sub.start;
  }
  return sub.segments[index - 1]?.to ?? sub.start;
}

/** Number of distinct nodes in a subpath (closing segment adds none). */
export function nodeCount(sub: SubPath): number {
  return sub.closed ? sub.segments.length : sub.segments.length + 1;
}

function nodePoint(sub: SubPath, node: number): Point {
  return node === 0 ? sub.start : (sub.segments[node - 1]?.to ?? sub.start);
}

function isSmooth(sub: SubPath, node: number): boolean {
  const count = sub.segments.length;
  const incomingIndex = node === 0 ? (sub.closed ? count - 1 : -1) : node - 1;
  const outgoingIndex = node < count ? node : sub.closed ? 0 : -1;
  const incoming = incomingIndex >= 0 ? sub.segments[incomingIndex] : undefined;
  const outgoing = outgoingIndex >= 0 ? sub.segments[outgoingIndex] : undefined;
  if (incoming?.kind !== 'cubic' || outgoing?.kind !== 'cubic') {
    return false;
  }
  const p = nodePoint(sub, node);
  const a = { x: p.x - incoming.c2.x, y: p.y - incoming.c2.y };
  const b = { x: outgoing.c1.x - p.x, y: outgoing.c1.y - p.y };
  const cross = a.x * b.y - a.y * b.x;
  const dot = a.x * b.x + a.y * b.y;
  return Math.abs(cross) < 1e-6 * (Math.hypot(a.x, a.y) * Math.hypot(b.x, b.y) + 1e-9) && dot > 0;
}

export function listNodes(path: Path): PathNode[] {
  const nodes: PathNode[] = [];
  path.subpaths.forEach((sub, subpath) => {
    for (let node = 0; node < nodeCount(sub); node += 1) {
      const p = nodePoint(sub, node);
      nodes.push({ subpath, node, x: p.x, y: p.y, smooth: isSmooth(sub, node), start: node === 0 });
    }
  });
  return nodes;
}

export function listHandles(path: Path): ControlHandle[] {
  const handles: ControlHandle[] = [];
  path.subpaths.forEach((sub, subpath) => {
    sub.segments.forEach((segment, index) => {
      if (segment.kind !== 'cubic') {
        return;
      }
      const from = segmentStart(sub, index);
      handles.push({
        subpath,
        segment: index,
        which: 'c1',
        x: segment.c1.x,
        y: segment.c1.y,
        anchorX: from.x,
        anchorY: from.y,
      });
      handles.push({
        subpath,
        segment: index,
        which: 'c2',
        x: segment.c2.x,
        y: segment.c2.y,
        anchorX: segment.to.x,
        anchorY: segment.to.y,
      });
    });
  });
  return handles;
}

/** Nearest segment of the path to a point, with the nearest point on it. */
export function nearestSegment(
  path: Path,
  p: Point,
  tolerance: number,
): { ref: SegmentRef; point: Point; dist: number; t: number } | null {
  let best: { ref: SegmentRef; point: Point; dist: number; t: number } | null = null;
  path.subpaths.forEach((sub, subpath) => {
    sub.segments.forEach((segment, index) => {
      const from = segmentStart(sub, index);
      const flat: number[] = [from.x, from.y];
      flattenSegment(from, segment, tolerance, flat);
      const pieces = flat.length / 2 - 1;
      for (let i = 0; i < pieces; i += 1) {
        const hit = nearestOnSegment(
          flat[2 * i] ?? 0,
          flat[2 * i + 1] ?? 0,
          flat[2 * i + 2] ?? 0,
          flat[2 * i + 3] ?? 0,
          p.x,
          p.y,
        );
        if (best === null || hit.dist < best.dist) {
          best = {
            ref: { subpath, segment: index },
            point: { x: hit.x, y: hit.y },
            dist: hit.dist,
            t: (i + hit.t) / pieces,
          };
        }
      }
    });
  });
  return best;
}

/** Moves a node, dragging attached cubic handles with it. */
export function moveNode(path: Path, ref: NodeRef, to: Point): Path {
  const next = clone(path);
  const sub = next.subpaths[ref.subpath];
  if (sub === undefined) {
    return next;
  }
  const before = nodePoint(sub, ref.node);
  const dx = to.x - before.x;
  const dy = to.y - before.y;
  const count = sub.segments.length;
  const setPoint = (index: number, point: Point) => {
    const segment = sub.segments[index];
    if (segment !== undefined) {
      segment.to = point;
      if (segment.kind === 'cubic') {
        segment.c2 = { x: segment.c2.x + dx, y: segment.c2.y + dy };
      }
    }
  };
  if (ref.node === 0) {
    sub.start = { ...to };
    if (sub.closed && count > 0) {
      setPoint(count - 1, { ...to });
    }
  } else {
    setPoint(ref.node - 1, { ...to });
  }
  const outgoingIndex = ref.node < count ? ref.node : sub.closed ? 0 : -1;
  const outgoing = outgoingIndex >= 0 ? sub.segments[outgoingIndex] : undefined;
  if (outgoing?.kind === 'cubic') {
    outgoing.c1 = { x: outgoing.c1.x + dx, y: outgoing.c1.y + dy };
  }
  return next;
}

/** Moves one bezier control handle; the opposite handle follows on smooth nodes. */
export function moveHandle(path: Path, handle: ControlHandle, to: Point): Path {
  const next = clone(path);
  const sub = next.subpaths[handle.subpath];
  const segment = sub?.segments[handle.segment];
  if (sub === undefined || segment === undefined || segment.kind !== 'cubic') {
    return next;
  }
  if (handle.which === 'c1') {
    segment.c1 = { ...to };
    const node = handle.segment;
    if (isSmooth(sub, node)) {
      const p = nodePoint(sub, node);
      const prevIndex = node === 0 ? sub.segments.length - 1 : node - 1;
      const prev = sub.segments[prevIndex];
      if (prev?.kind === 'cubic') {
        const length = Math.hypot(prev.c2.x - p.x, prev.c2.y - p.y);
        const dir = { x: to.x - p.x, y: to.y - p.y };
        const norm = Math.hypot(dir.x, dir.y) || 1;
        prev.c2 = { x: p.x - (dir.x / norm) * length, y: p.y - (dir.y / norm) * length };
      }
    }
  } else {
    segment.c2 = { ...to };
    const node = handle.segment + 1;
    const nextIndex = node < sub.segments.length ? node : sub.closed ? 0 : -1;
    const following = nextIndex >= 0 ? sub.segments[nextIndex] : undefined;
    if (following?.kind === 'cubic' && isSmooth(sub, node % Math.max(1, nodeCount(sub)))) {
      const p = segment.to;
      const length = Math.hypot(following.c1.x - p.x, following.c1.y - p.y);
      const dir = { x: to.x - p.x, y: to.y - p.y };
      const norm = Math.hypot(dir.x, dir.y) || 1;
      following.c1 = { x: p.x - (dir.x / norm) * length, y: p.y - (dir.y / norm) * length };
    }
  }
  return next;
}

function splitSegment(from: Point, segment: Segment, t: number): [Segment, Segment] {
  switch (segment.kind) {
    case 'line': {
      const mid = {
        x: from.x + (segment.to.x - from.x) * t,
        y: from.y + (segment.to.y - from.y) * t,
      };
      return [
        { kind: 'line', to: mid },
        { kind: 'line', to: segment.to },
      ];
    }
    case 'arc': {
      const arc = arcFromBulge(from, segment.to, segment.bulge);
      if (arc === null) {
        const mid = {
          x: from.x + (segment.to.x - from.x) * t,
          y: from.y + (segment.to.y - from.y) * t,
        };
        return [
          { kind: 'line', to: mid },
          { kind: 'line', to: segment.to },
        ];
      }
      const mid = pointOnArc(arc, t);
      return [
        { kind: 'arc', to: mid, bulge: Math.tan((arc.sweep * t) / 4) },
        { kind: 'arc', to: segment.to, bulge: Math.tan((arc.sweep * (1 - t)) / 4) },
      ];
    }
    case 'cubic': {
      const lerp = (a: Point, b: Point): Point => ({
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
      });
      const p01 = lerp(from, segment.c1);
      const p12 = lerp(segment.c1, segment.c2);
      const p23 = lerp(segment.c2, segment.to);
      const p012 = lerp(p01, p12);
      const p123 = lerp(p12, p23);
      const mid = lerp(p012, p123);
      return [
        { kind: 'cubic', c1: p01, c2: p012, to: mid },
        { kind: 'cubic', c1: p123, c2: p23, to: segment.to },
      ];
    }
  }
}

/** Inserts a node on a segment at parameter t (0..1). */
export function insertNode(path: Path, ref: SegmentRef, t: number): Path {
  const next = clone(path);
  const sub = next.subpaths[ref.subpath];
  const segment = sub?.segments[ref.segment];
  if (sub === undefined || segment === undefined) {
    return next;
  }
  const from = segmentStart(sub, ref.segment);
  const [first, second] = splitSegment(from, segment, Math.min(0.999, Math.max(0.001, t)));
  sub.segments.splice(ref.segment, 1, first, second);
  return next;
}

/** Deletes a node; its neighbours join with the outgoing segment's kind. */
export function deleteNode(path: Path, ref: NodeRef): Path {
  const next = clone(path);
  const sub = next.subpaths[ref.subpath];
  if (sub === undefined || nodeCount(sub) <= 2) {
    return next;
  }
  if (ref.node === 0) {
    const first = sub.segments.shift();
    if (first === undefined) {
      return next;
    }
    sub.start = first.to;
    if (sub.closed) {
      const last = sub.segments[sub.segments.length - 1];
      if (last !== undefined) {
        last.to = sub.start;
        if (last.kind === 'cubic') {
          last.c2 = { x: (last.c2.x + sub.start.x) / 2, y: (last.c2.y + sub.start.y) / 2 };
        }
      }
    }
    return next;
  }
  const incoming = sub.segments[ref.node - 1];
  const outgoing = sub.segments[ref.node];
  if (incoming === undefined) {
    return next;
  }
  if (outgoing === undefined) {
    sub.segments.pop();
    return next;
  }
  const merged: Segment =
    incoming.kind === 'cubic' && outgoing.kind === 'cubic'
      ? { kind: 'cubic', c1: incoming.c1, c2: outgoing.c2, to: outgoing.to }
      : incoming.kind === 'arc' && outgoing.kind === 'arc'
        ? {
            kind: 'arc',
            to: outgoing.to,
            bulge: Math.tan((4 * Math.atan(incoming.bulge) + 4 * Math.atan(outgoing.bulge)) / 4),
          }
        : { kind: 'line', to: outgoing.to };
  sub.segments.splice(ref.node - 1, 2, merged);
  return next;
}

/** Converts a segment to another kind, keeping its end points. */
export function convertSegment(path: Path, ref: SegmentRef, to: 'line' | 'arc' | 'cubic'): Path {
  const next = clone(path);
  const sub = next.subpaths[ref.subpath];
  const segment = sub?.segments[ref.segment];
  if (sub === undefined || segment === undefined || segment.kind === to) {
    return next;
  }
  const from = segmentStart(sub, ref.segment);
  const end = segment.to;
  let replacement: Segment;
  switch (to) {
    case 'line':
      replacement = { kind: 'line', to: end };
      break;
    case 'arc':
      replacement = { kind: 'arc', to: end, bulge: Math.tan(Math.PI / 8) };
      break;
    case 'cubic':
      replacement = {
        kind: 'cubic',
        c1: { x: from.x + (end.x - from.x) / 3, y: from.y + (end.y - from.y) / 3 },
        c2: { x: from.x + ((end.x - from.x) * 2) / 3, y: from.y + ((end.y - from.y) * 2) / 3 },
        to: end,
      };
      break;
  }
  sub.segments[ref.segment] = replacement;
  return next;
}

/** Makes the node smooth (aligns its cubic handles) or sharp (toggles). */
export function toggleSmooth(path: Path, ref: NodeRef): Path {
  const next = clone(path);
  const sub = next.subpaths[ref.subpath];
  if (sub === undefined) {
    return next;
  }
  const count = sub.segments.length;
  const incomingIndex = ref.node === 0 ? (sub.closed ? count - 1 : -1) : ref.node - 1;
  const outgoingIndex = ref.node < count ? ref.node : sub.closed ? 0 : -1;
  if (incomingIndex < 0 || outgoingIndex < 0) {
    return next;
  }
  const p = nodePoint(sub, ref.node);
  const from = segmentStart(sub, incomingIndex);
  const outgoing = sub.segments[outgoingIndex];
  const incoming = sub.segments[incomingIndex];
  if (incoming === undefined || outgoing === undefined) {
    return next;
  }
  if (isSmooth(sub, ref.node)) {
    // Break the tangent by pulling the handles halfway to the node.
    if (incoming.kind === 'cubic') {
      incoming.c2 = { x: (incoming.c2.x + p.x) / 2, y: (incoming.c2.y + p.y) / 2 + 0.01 };
    }
    return next;
  }
  const inCubic: Segment =
    incoming.kind === 'cubic'
      ? incoming
      : {
          kind: 'cubic',
          c1: { x: from.x + (p.x - from.x) / 3, y: from.y + (p.y - from.y) / 3 },
          c2: { x: from.x + ((p.x - from.x) * 2) / 3, y: from.y + ((p.y - from.y) * 2) / 3 },
          to: p,
        };
  const outCubic: Segment =
    outgoing.kind === 'cubic'
      ? outgoing
      : {
          kind: 'cubic',
          c1: { x: p.x + (outgoing.to.x - p.x) / 3, y: p.y + (outgoing.to.y - p.y) / 3 },
          c2: {
            x: p.x + ((outgoing.to.x - p.x) * 2) / 3,
            y: p.y + ((outgoing.to.y - p.y) * 2) / 3,
          },
          to: outgoing.to,
        };
  if (inCubic.kind !== 'cubic' || outCubic.kind !== 'cubic') {
    return next;
  }
  const tangent = { x: outCubic.to.x - from.x, y: outCubic.to.y - from.y };
  const norm = Math.hypot(tangent.x, tangent.y) || 1;
  const inLength = Math.hypot(p.x - from.x, p.y - from.y) / 3;
  const outLength = Math.hypot(outCubic.to.x - p.x, outCubic.to.y - p.y) / 3;
  inCubic.c2 = { x: p.x - (tangent.x / norm) * inLength, y: p.y - (tangent.y / norm) * inLength };
  outCubic.c1 = {
    x: p.x + (tangent.x / norm) * outLength,
    y: p.y + (tangent.y / norm) * outLength,
  };
  sub.segments[incomingIndex] = inCubic;
  sub.segments[outgoingIndex] = outCubic;
  return next;
}

/** Makes the node the start point of a closed subpath. */
export function setStartNode(path: Path, ref: NodeRef): Path {
  const next = clone(path);
  const sub = next.subpaths[ref.subpath];
  if (sub === undefined || !sub.closed || ref.node === 0) {
    return next;
  }
  const points = Array.from({ length: nodeCount(sub) }, (_unused, i) => nodePoint(sub, i));
  sub.start = points[ref.node] ?? sub.start;
  sub.segments = [...sub.segments.slice(ref.node), ...sub.segments.slice(0, ref.node)];
  return next;
}

/** Opens a closed subpath at the node, or splits an open one into two. */
export function cutAtNode(path: Path, ref: NodeRef): Path {
  const next = clone(path);
  const sub = next.subpaths[ref.subpath];
  if (sub === undefined) {
    return next;
  }
  if (sub.closed) {
    const opened = setStartNode({ subpaths: [sub] }, { subpath: 0, node: ref.node }).subpaths[0];
    if (opened !== undefined) {
      opened.closed = false;
      next.subpaths[ref.subpath] = opened;
    }
    return next;
  }
  if (ref.node === 0 || ref.node >= nodeCount(sub) - 1) {
    return next;
  }
  const cutPoint = nodePoint(sub, ref.node);
  const first: SubPath = {
    start: sub.start,
    closed: false,
    segments: sub.segments.slice(0, ref.node),
  };
  const second: SubPath = {
    start: cutPoint,
    closed: false,
    segments: sub.segments.slice(ref.node),
  };
  next.subpaths.splice(ref.subpath, 1, first, second);
  return next;
}

/** Moves several nodes by the same offset. */
export function moveNodes(path: Path, refs: readonly NodeRef[], by: Point): Path {
  let next = path;
  for (const ref of refs) {
    const sub = next.subpaths[ref.subpath];
    if (sub === undefined) {
      continue;
    }
    const at = nodePoint(sub, ref.node);
    next = moveNode(next, ref, { x: at.x + by.x, y: at.y + by.y });
  }
  return next;
}

/** X / Y: lines the nodes up with the first one's X or Y. */
export function alignNodes(path: Path, refs: readonly NodeRef[], axis: 'x' | 'y'): Path {
  const [first, ...rest] = refs;
  const sub = first === undefined ? undefined : path.subpaths[first.subpath];
  if (first === undefined || sub === undefined) {
    return path;
  }
  const target = nodePoint(sub, first.node);
  let next = path;
  for (const ref of rest) {
    const own = next.subpaths[ref.subpath];
    if (own === undefined) {
      continue;
    }
    const at = nodePoint(own, ref.node);
    next = moveNode(next, ref, axis === 'x' ? { x: target.x, y: at.y } : { x: at.x, y: target.y });
  }
  return next;
}

/** Inserts a node half way along a span. */
export function insertMidpoint(path: Path, ref: SegmentRef): Path {
  return insertNode(path, ref, 0.5);
}

/**
 * Deletes a span: a closed shape opens there, an open vector splits in two
 * (an end span simply goes).
 */
export function deleteSpan(path: Path, ref: SegmentRef): Path {
  const next = clone(path);
  const sub = next.subpaths[ref.subpath];
  if (sub === undefined || ref.segment >= sub.segments.length) {
    return next;
  }
  if (sub.closed) {
    const count = sub.segments.length;
    const reordered = setStartNode(
      { subpaths: [sub] },
      { subpath: 0, node: (ref.segment + 1) % count },
    ).subpaths[0];
    if (reordered !== undefined) {
      reordered.segments = reordered.segments.slice(0, -1);
      reordered.closed = false;
      next.subpaths[ref.subpath] = reordered;
    }
    return next;
  }
  const first: SubPath = {
    start: sub.start,
    closed: false,
    segments: sub.segments.slice(0, ref.segment),
  };
  const second: SubPath = {
    start: nodePoint(sub, ref.segment + 1),
    closed: false,
    segments: sub.segments.slice(ref.segment + 1),
  };
  const parts = [first, second].filter((part) => part.segments.length > 0);
  next.subpaths.splice(ref.subpath, 1, ...parts);
  return next;
}

/**
 * Drags a span by the point at `t` along it: an arc bends to pass through
 * the new point, a curve's control points follow so the dragged point
 * does, and a line (or any span with `whole`, Ctrl) moves with both nodes.
 */
export function dragSpan(
  path: Path,
  ref: SegmentRef,
  t: number,
  by: Point,
  whole: boolean,
  keepTangency = false,
): Path {
  const sub = path.subpaths[ref.subpath];
  const segment = sub?.segments[ref.segment];
  if (sub === undefined || segment === undefined) {
    return path;
  }
  const count = nodeCount(sub);
  const endNode = sub.closed ? (ref.segment + 1) % count : ref.segment + 1;
  if (whole || segment.kind === 'line') {
    return moveNodes(
      path,
      [
        { subpath: ref.subpath, node: ref.segment },
        { subpath: ref.subpath, node: endNode },
      ],
      by,
    );
  }
  const next = clone(path);
  const own = next.subpaths[ref.subpath];
  const span = own?.segments[ref.segment];
  if (own === undefined || span === undefined) {
    return next;
  }
  const from = segmentStart(own, ref.segment);
  if (span.kind === 'arc') {
    const arc = arcFromBulge(from, span.to, span.bulge);
    const on = arc === null ? null : pointOnArc(arc, 0.5);
    if (on !== null) {
      const through = { x: on.x + by.x, y: on.y + by.y };
      // The bulge of the arc through start, the moved middle and end.
      const chord = { x: span.to.x - from.x, y: span.to.y - from.y };
      const length = Math.hypot(chord.x, chord.y);
      if (length > 1e-9) {
        const mid = { x: (from.x + span.to.x) / 2, y: (from.y + span.to.y) / 2 };
        // Signed rise of the dragged middle above the chord (left positive).
        const rise = (chord.x * (through.y - mid.y) - chord.y * (through.x - mid.x)) / length;
        span.bulge = (-2 * rise) / length;
      }
    }
    return next;
  }
  if (span.kind === 'cubic' && keepTangency) {
    // K mode: each control point slides along its own direction only, so the
    // span leaves and meets its nodes at the same angles.
    const u1 = unitOr(
      { x: span.c1.x - from.x, y: span.c1.y - from.y },
      { x: span.to.x - from.x, y: span.to.y - from.y },
    );
    const u2 = unitOr(
      { x: span.c2.x - span.to.x, y: span.c2.y - span.to.y },
      { x: from.x - span.to.x, y: from.y - span.to.y },
    );
    const at = Math.min(0.95, Math.max(0.05, t));
    const w1 = 3 * (1 - at) * (1 - at) * at;
    const w2 = 3 * (1 - at) * at * at;
    // Solve w1 a u1 + w2 b u2 = by for the slide lengths a and b.
    const det = w1 * w2 * (u1.x * u2.y - u1.y * u2.x);
    let a: number;
    let b: number;
    if (Math.abs(det) > 1e-9) {
      a = (by.x * w2 * u2.y - by.y * w2 * u2.x) / det;
      b = (w1 * u1.x * by.y - w1 * u1.y * by.x) / det;
    } else {
      // Parallel handles: share the part of the move along them.
      const along = (by.x * u1.x + by.y * u1.y) / (w1 + w2);
      a = along;
      b = along * Math.sign(u1.x * u2.x + u1.y * u2.y);
    }
    span.c1 = { x: span.c1.x + a * u1.x, y: span.c1.y + a * u1.y };
    span.c2 = { x: span.c2.x + b * u2.x, y: span.c2.y + b * u2.y };
    return next;
  }
  if (span.kind === 'cubic') {
    const weight = Math.max(0.05, 3 * t * (1 - t));
    const shift = { x: by.x / weight / 2, y: by.y / weight / 2 };
    span.c1 = { x: span.c1.x + shift.x, y: span.c1.y + shift.y };
    span.c2 = { x: span.c2.x + shift.x, y: span.c2.y + shift.y };
  }
  return next;
}

function unitOr(v: Point, fallback: Point): Point {
  const length = Math.hypot(v.x, v.y);
  if (length > 1e-9) {
    return { x: v.x / length, y: v.y / length };
  }
  const other = Math.hypot(fallback.x, fallback.y);
  return other > 1e-9 ? { x: fallback.x / other, y: fallback.y / other } : { x: 1, y: 0 };
}

/** H / V mirror mode: the node lying across the vector's centre line from `ref`, if any. */
export function mirrorNode(path: Path, ref: NodeRef, axis: 'h' | 'v'): NodeRef | null {
  const nodes = listNodes(path);
  const xs = nodes.map((n) => n.x);
  const ys = nodes.map((n) => n.y);
  const cx = (Math.min(...xs) + Math.max(...xs)) / 2;
  const cy = (Math.min(...ys) + Math.max(...ys)) / 2;
  const self = nodes.find((n) => n.subpath === ref.subpath && n.node === ref.node);
  if (self === undefined) {
    return null;
  }
  const want = axis === 'h' ? { x: 2 * cx - self.x, y: self.y } : { x: self.x, y: 2 * cy - self.y };
  const scale = Math.max(1, Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys));
  const match = nodes.find(
    (n) =>
      !(n.subpath === ref.subpath && n.node === ref.node) &&
      Math.hypot(n.x - want.x, n.y - want.y) <= scale * 1e-6,
  );
  return match === undefined ? null : { subpath: match.subpath, node: match.node };
}

/** One segment run the other way, ending at `to`. */
function reversedSegment(segment: Segment, to: Point): Segment {
  switch (segment.kind) {
    case 'line':
      return { kind: 'line', to };
    case 'arc':
      return { kind: 'arc', to, bulge: -segment.bulge };
    default:
      return { kind: 'cubic', c1: segment.c2, c2: segment.c1, to };
  }
}

/** Reverse Vector Direction: every subpath runs the other way. */
export function reversePath(path: Path): Path {
  return {
    subpaths: path.subpaths.map((sub) => {
      const points = [sub.start, ...sub.segments.map((s) => s.to)];
      const n = sub.segments.length;
      const segments: Segment[] = [];
      for (let j = 0; j < n; j += 1) {
        const segment = sub.segments[n - 1 - j];
        const to = points[n - 1 - j];
        if (segment !== undefined && to !== undefined) {
          segments.push(reversedSegment(segment, to));
        }
      }
      return { start: points[n] ?? sub.start, closed: sub.closed, segments };
    }),
  };
}

/** Close Vector: an open subpath (the one given, else every one) is closed
 * with a straight span back to its start. */
export function closeSubpaths(path: Path, only?: number): Path {
  const next = clone(path);
  next.subpaths.forEach((sub, index) => {
    if (sub.closed || (only !== undefined && index !== only)) {
      return;
    }
    const end = sub.segments[sub.segments.length - 1]?.to ?? sub.start;
    if (Math.hypot(end.x - sub.start.x, end.y - sub.start.y) > 1e-9) {
      sub.segments.push({ kind: 'line', to: { ...sub.start } });
    }
    sub.closed = sub.segments.length > 0;
  });
  return next;
}

/** Where a node is. */
export function nodePosition(path: Path, ref: NodeRef): Point | null {
  const sub = path.subpaths[ref.subpath];
  return sub === undefined ? null : nodePoint(sub, ref.node);
}
