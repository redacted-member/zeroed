// Line segments the 3D view draws over the model: toolpath moves and 2D
// vectors. Each result is flat x, y, z pairs of segment ends in scene axes
// (job x -> x, job y -> -z, job z -> y with the material top at y zero).

import type { MovesReply } from '../../bridge/cam';
import type { JobSetup } from '../../bridge/types';
import { ALL_MOVES, moveGroup, type MoveGroup, type MoveVisibility } from '../../lib/moveGroups';
import { rotaryOf, wrapPoint } from '../../lib/rotary';

/** The largest turn one drawn piece of a wrapped move makes, radians. A
 * straight flat move is a helix on the blank, so long ones are cut up. */
const WRAP_STEP = (5 * Math.PI) / 180;

/** The most pieces one wrapped move is drawn in. */
const MAX_WRAP_PIECES = 720;

/** Segments of each move group: cutting and arc moves, rapids, plunges,
 * retracts and joins. */
export type ToolpathLines = Record<MoveGroup, Float32Array>;

/**
 * Segments for every visible toolpath in `moves`, in the groups `shown`
 * allows. `surfaceZ` is the job Z of the material top (0 when Z zero is on
 * the top, the thickness when it is on the bottom).
 */
export function toolpathLines(
  moves: MovesReply,
  surfaceZ: number,
  shown: MoveVisibility = ALL_MOVES,
): ToolpathLines {
  const out: Record<MoveGroup, number[]> = {
    cut: [],
    rapid: [],
    plunge: [],
    retract: [],
    join: [],
  };
  const { xyz, kind } = moves;
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
      const a = (i - 1) * 3;
      const b = i * 3;
      out[group].push(
        xyz[a] ?? 0,
        (xyz[a + 2] ?? 0) - surfaceZ,
        -(xyz[a + 1] ?? 0),
        xyz[b] ?? 0,
        (xyz[b + 2] ?? 0) - surfaceZ,
        -(xyz[b + 1] ?? 0),
      );
    }
  }
  return {
    cut: new Float32Array(out.cut),
    rapid: new Float32Array(out.rapid),
    plunge: new Float32Array(out.plunge),
    retract: new Float32Array(out.retract),
    join: new Float32Array(out.join),
  };
}

/**
 * Segments for every visible toolpath on a rotary job, each move where the
 * machine takes it: Z from the job's zero, so its distance from the axis is
 * the radius plus its height over the surface, and the distance across the
 * axis turned into an angle. Long moves come out as many short pieces.
 */
export function wrappedToolpathLines(
  moves: MovesReply,
  job: JobSetup,
  surfaceZ: number,
  shown: MoveVisibility = ALL_MOVES,
): ToolpathLines {
  const out: Record<MoveGroup, number[]> = {
    cut: [],
    rapid: [],
    plunge: [],
    retract: [],
    join: [],
  };
  const { axis, diameter } = rotaryOf(job);
  const radius = diameter / 2;
  const alongX = axis === 'x';
  const { xyz, kind } = moves;
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
      const a = (i - 1) * 3;
      const b = i * 3;
      const [ax, ay, az] = [xyz[a] ?? 0, xyz[a + 1] ?? 0, xyz[a + 2] ?? 0];
      const [bx, by, bz] = [xyz[b] ?? 0, xyz[b + 1] ?? 0, xyz[b + 2] ?? 0];
      const [startAlong, startAround] = alongX ? [ax, ay] : [ay, ax];
      const [endAlong, endAround] = alongX ? [bx, by] : [by, bx];
      const turn = radius > 0 ? Math.abs(endAround - startAround) / radius : 0;
      const pieces = Math.min(MAX_WRAP_PIECES, Math.max(1, Math.ceil(turn / WRAP_STEP)));
      const at = (t: number) => {
        const radial = radius + az + (bz - az) * t - surfaceZ;
        return wrapPoint(
          axis,
          diameter,
          startAlong + (endAlong - startAlong) * t,
          startAround + (endAround - startAround) * t,
          Math.max(0, radial),
        );
      };
      let from = at(0);
      for (let piece = 1; piece <= pieces; piece += 1) {
        const to = at(piece / pieces);
        out[group].push(from.x, from.y, from.z, to.x, to.y, to.z);
        from = to;
      }
    }
  }
  return {
    cut: new Float32Array(out.cut),
    rapid: new Float32Array(out.rapid),
    plunge: new Float32Array(out.plunge),
    retract: new Float32Array(out.retract),
    join: new Float32Array(out.join),
  };
}

/** One vector's flattened runs (flat x, y pairs) and whether it is closed. */
export interface VectorRuns {
  runs: readonly Float32Array[];
  closed: boolean;
}

/** Segments for vectors lying flat at scene height `sceneY`. */
export function vectorLines(vectors: readonly VectorRuns[], sceneY: number): Float32Array {
  const out: number[] = [];
  const push = (run: Float32Array, a: number, b: number) => {
    out.push(run[a] ?? 0, sceneY, -(run[a + 1] ?? 0), run[b] ?? 0, sceneY, -(run[b + 1] ?? 0));
  };
  for (const vector of vectors) {
    for (const run of vector.runs) {
      const points = Math.floor(run.length / 2);
      for (let i = 1; i < points; i += 1) {
        push(run, (i - 1) * 2, i * 2);
      }
      if (vector.closed && points > 2) {
        push(run, (points - 1) * 2, 0);
      }
    }
  }
  return new Float32Array(out);
}
