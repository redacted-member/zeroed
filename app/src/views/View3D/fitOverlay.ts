// What the 3D view draws for Verify Fit: the faces of each joint facing
// each other, colored by how they fit (red where the pieces run into each
// other, amber tight, green within the tolerance, blue loose). The picked
// joint shows strong; Show Fit shows every joint at once, paler. Faces are
// in each piece's own frame, so they follow the pieces as they explode.

import { pieceKey, type PieceRow } from '../../bridge/assembly';
import type { FitJoint, FitStatus } from '../../bridge/assemblyFit';
import { pieceLocalSceneMatrix, type Mat4 } from './pieceGizmo';

/** A patch of faces to draw: triangles in a piece's frame and its matrix to the scene. */
export interface FitMesh {
  triangles: Float32Array;
  matrix: Mat4;
  colour: number;
  /** The picked joint, drawn strong. */
  strong: boolean;
}

export const FIT_COLOURS: Readonly<Record<FitStatus, number>> = {
  interference: 0xd93b3b,
  tight: 0xe8a317,
  ok: 0x2fa84f,
  loose: 0x3d7fe0,
};

/** The meshes for the picked joint and, with Show Fit, every joint. */
export function fitMeshes(
  joints: readonly FitJoint[] | null,
  pieces: readonly PieceRow[],
  picked: number | null,
  showFit: boolean,
): FitMesh[] {
  if (joints === null) {
    return [];
  }
  const matrices = new Map<string, Mat4>();
  for (const piece of pieces) {
    matrices.set(pieceKey(piece), pieceLocalSceneMatrix(piece));
  }
  const out: FitMesh[] = [];
  joints.forEach((joint, index) => {
    const strong = index === picked;
    if (!strong && !showFit) {
      return;
    }
    const a = matrices.get(pieceKey(joint.a));
    const b = matrices.get(pieceKey(joint.b));
    for (const patch of joint.patches) {
      const colour = FIT_COLOURS[patch.status];
      if (a !== undefined && patch.a.length > 0) {
        out.push({ triangles: new Float32Array(patch.a), matrix: a, colour, strong });
      }
      if (b !== undefined && patch.b.length > 0) {
        out.push({ triangles: new Float32Array(patch.b), matrix: b, colour, strong });
      }
    }
  });
  return out;
}

/**
 * The box to frame a joint on, in job space: its bounds moved as far as
 * its pieces are exploded, and at least `least` mm across.
 */
export function jointFrame(
  joint: FitJoint,
  shift: [number, number, number],
  least = 40,
): { min: [number, number, number]; max: [number, number, number] } {
  const min: [number, number, number] = [0, 0, 0];
  const max: [number, number, number] = [0, 0, 0];
  for (let k = 0; k < 3; k += 1) {
    const lo = (joint.bounds[0][k] ?? 0) + shift[k];
    const hi = (joint.bounds[1][k] ?? 0) + shift[k];
    const mid = (lo + hi) / 2;
    const half = Math.max((hi - lo) / 2, least / 2);
    min[k] = mid - half;
    max[k] = mid + half;
  }
  return { min, max };
}
