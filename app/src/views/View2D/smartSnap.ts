// Smart snapping: infers horizontal and vertical alignment lines from key
// points of nearby objects (bounding box corners and centres, nodes) and
// snaps the cursor onto them. Pure so it can be tested without a canvas.

import type { Bbox } from '../../bridge/document';
import type { Point2 } from './camera';

/** One inferred alignment line, drawn dashed from `from` to `to`. */
export interface AlignmentLine {
  /** 'x' when the cursor shares the source's x (a vertical line). */
  axis: 'x' | 'y';
  from: Point2;
  to: Point2;
}

export interface AlignmentSnap {
  point: Point2;
  lines: AlignmentLine[];
}

/** The part of a display entry the finder reads. */
export interface AlignmentSource {
  id: number;
  layer: number;
  bbox: Bbox;
  /** Flat x, y pairs per run. */
  runs: readonly Float32Array[];
}

/** Runs with more vertices than this are curves flattened for display, whose
 * vertices are not real nodes; only their start points are used. */
const MAX_NODE_VERTICES = 64;

/**
 * Key points of the objects whose bounding box lies within `reach` of `near`:
 * the four corners and the centre of each box, plus node points.
 */
export function alignmentCandidates(
  sources: readonly AlignmentSource[],
  near: Point2,
  reach: number,
  skip: (source: AlignmentSource) => boolean = () => false,
): Point2[] {
  const points: Point2[] = [];
  for (const source of sources) {
    const [minX, minY, maxX, maxY] = source.bbox;
    if (!Number.isFinite(minX) || !Number.isFinite(maxX) || skip(source)) {
      continue;
    }
    const dx = Math.max(minX - near.x, 0, near.x - maxX);
    const dy = Math.max(minY - near.y, 0, near.y - maxY);
    if (Math.hypot(dx, dy) > reach) {
      continue;
    }
    points.push(
      { x: minX, y: minY },
      { x: maxX, y: minY },
      { x: minX, y: maxY },
      { x: maxX, y: maxY },
      { x: (minX + maxX) / 2, y: (minY + maxY) / 2 },
    );
    for (const run of source.runs) {
      const count = Math.floor(run.length / 2);
      const limit = count > MAX_NODE_VERTICES ? 1 : count;
      for (let i = 0; i < limit; i += 1) {
        points.push({ x: run[i * 2] ?? 0, y: run[i * 2 + 1] ?? 0 });
      }
    }
  }
  return points;
}

/** The candidate aligned on one axis closest to the cursor, nearest first on ties. */
function bestOnAxis(
  at: Point2,
  candidates: readonly Point2[],
  tolerance: number,
  axis: 'x' | 'y',
): Point2 | null {
  let best: Point2 | null = null;
  let bestOff = Infinity;
  let bestDist = Infinity;
  for (const c of candidates) {
    const off = Math.abs(c[axis] - at[axis]);
    if (off > tolerance) {
      continue;
    }
    const dist = Math.hypot(c.x - at.x, c.y - at.y);
    if (off < bestOff - 1e-9 || (Math.abs(off - bestOff) <= 1e-9 && dist < bestDist)) {
      best = c;
      bestOff = off;
      bestDist = dist;
    }
  }
  return best;
}

/**
 * Snaps `at` onto the alignment lines within `tolerance` (mm). Each axis is
 * matched independently, so the cursor can sit where a vertical line from
 * one object crosses a horizontal line from another. Answers null when
 * nothing aligns.
 */
export function findAlignment(
  at: Point2,
  candidates: readonly Point2[],
  tolerance: number,
): AlignmentSnap | null {
  if (!(tolerance > 0)) {
    return null;
  }
  const vertical = bestOnAxis(at, candidates, tolerance, 'x');
  const horizontal = bestOnAxis(at, candidates, tolerance, 'y');
  if (vertical === null && horizontal === null) {
    return null;
  }
  const point = { x: vertical?.x ?? at.x, y: horizontal?.y ?? at.y };
  const lines: AlignmentLine[] = [];
  if (vertical !== null) {
    lines.push({ axis: 'x', from: vertical, to: point });
  }
  if (horizontal !== null) {
    lines.push({ axis: 'y', from: horizontal, to: point });
  }
  return { point, lines };
}
