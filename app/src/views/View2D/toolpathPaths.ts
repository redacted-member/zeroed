// Toolpaths as the 2D view draws them, seen from above: segments per move
// group (plunges and retracts only show when they move sideways), filtered by
// Toolpaths > Toolpath Drawing, and for Make 2D Previews Solid the cutting
// moves bucketed by the width the tool cuts, so each bucket strokes once at
// that width.

import type { MovesReply } from '../../bridge/cam';
import { MOVE_GROUPS, moveGroup, type MoveGroup, type MoveVisibility } from '../../lib/moveGroups';

/** Solid widths are rounded to this step, millimetres. */
export const WIDTH_STEP = 0.05;

export interface ToolpathPaths {
  /** Flat x0, y0, x1, y1 per segment, for each group. */
  lines: Record<MoveGroup, Float32Array>;
  /** Width in millimetres and the cutting segments cut at about that width. */
  solid: { width: number; segments: Float32Array }[];
}

/** The segments of the visible toolpaths the toggles show; `solid` fills the cut width. */
export function toolpathPaths(
  moves: MovesReply,
  shown: MoveVisibility,
  solid: boolean,
): ToolpathPaths {
  const lines: Record<MoveGroup, number[]> = {
    cut: [],
    rapid: [],
    plunge: [],
    retract: [],
    join: [],
  };
  const buckets = new Map<number, number[]>();
  const { xyz, kind, width } = moves;
  for (const toolpath of moves.toolpaths) {
    if (!toolpath.visible) {
      continue;
    }
    const end = toolpath.offset + toolpath.count;
    for (let i = toolpath.offset + 1; i < end; i += 1) {
      const group = moveGroup(kind[i]);
      if (group === null || !shown[group]) {
        continue;
      }
      const x0 = xyz[(i - 1) * 3] ?? 0;
      const y0 = xyz[(i - 1) * 3 + 1] ?? 0;
      const x1 = xyz[i * 3] ?? 0;
      const y1 = xyz[i * 3 + 1] ?? 0;
      if (x0 === x1 && y0 === y1) {
        continue;
      }
      lines[group].push(x0, y0, x1, y1);
      if (solid && group !== 'rapid' && group !== 'retract') {
        // The wider end of the move: a ramp widens as it goes down.
        const w = Math.max(width[i - 1] ?? 0, width[i] ?? 0);
        if (w > 0) {
          const key = Math.max(1, Math.round(w / WIDTH_STEP));
          let bucket = buckets.get(key);
          if (bucket === undefined) {
            bucket = [];
            buckets.set(key, bucket);
          }
          bucket.push(x0, y0, x1, y1);
        }
      }
    }
  }
  const out = {} as Record<MoveGroup, Float32Array>;
  for (const group of MOVE_GROUPS) {
    out[group] = new Float32Array(lines[group]);
  }
  return {
    lines: out,
    solid: [...buckets.entries()]
      .sort((a, b) => b[0] - a[0])
      .map(([key, segments]) => ({
        width: key * WIDTH_STEP,
        segments: new Float32Array(segments),
      })),
  };
}

/** How far from straight (the sine of the angle) a segment may turn and
 * still be drawn as part of the line before it. */
const STRAIGHT_ON = 1e-6;

/**
 * The segments (flat x0, y0, x1, y1) with each run of segments that carry
 * straight on from one another joined into one, as [x0, y0, x1, y1]: seen
 * from above, each raster line of a 3D finish is one straight line of
 * hundreds of moves, and drawing it as one looks the same.
 */
export function straightRuns(segments: Float32Array): [number, number, number, number][] {
  const runs: [number, number, number, number][] = [];
  let run: [number, number, number, number] | null = null;
  for (let i = 0; i + 3 < segments.length; i += 4) {
    const x0 = segments[i] ?? 0;
    const y0 = segments[i + 1] ?? 0;
    const x1 = segments[i + 2] ?? 0;
    const y1 = segments[i + 3] ?? 0;
    if (run !== null && run[2] === x0 && run[3] === y0) {
      const [ux, uy] = [run[2] - run[0], run[3] - run[1]];
      const [vx, vy] = [x1 - x0, y1 - y0];
      const lengths = Math.hypot(ux, uy) * Math.hypot(vx, vy);
      if (ux * vx + uy * vy > 0 && Math.abs(ux * vy - uy * vx) <= STRAIGHT_ON * lengths) {
        run[2] = x1;
        run[3] = y1;
        continue;
      }
    }
    if (run !== null) {
      runs.push(run);
    }
    run = [x0, y0, x1, y1];
  }
  if (run !== null) {
    runs.push(run);
  }
  return runs;
}
