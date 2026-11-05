// The maths of the Assembly View in the 3D view: where each piece's relief
// mesh goes, and how the move arrows, the rotation rings and the ball in the
// middle change a piece's place. Assembly space is job space (X, Y, Z up);
// the renderer's scene is (x, z, -y). A piece's pose puts the middle of its
// sheet's material box at `position`, turned by `rotation` (degrees about
// X, then Y, then Z), as the kernel places it.

import type { Heightfield } from '../../bridge/cam';
import type { PieceRow, Triple } from '../../bridge/assembly';
import {
  axisVector,
  turnedAbout,
  turnsMatrix,
  type Mat3,
  type RingAxis,
  type RingFrame,
  type Turns,
  type Vec3,
} from './rotateRings';

/** Arrows reach this far from the middle, as a share of the piece's largest side. */
const ARROW_SHARE = 0.45;
/** Rings are this big, as a share of the arrows. */
const RING_SHARE = 0.8;

export function turnsOfRotation(rotation: Triple): Turns {
  return { rotX: rotation[0], rotY: rotation[1], rotZ: rotation[2] };
}

export function rotationOfTurns(turns: Turns): Triple {
  return [turns.rotX, turns.rotY, turns.rotZ];
}

/** A column major 4 by 4 matrix, as three.js takes it. */
export type Mat4 = number[];

function affine(r: Mat3, t: Vec3): Mat4 {
  return [
    r[0][0],
    r[1][0],
    r[2][0],
    0,
    r[0][1],
    r[1][1],
    r[2][1],
    0,
    r[0][2],
    r[1][2],
    r[2][2],
    0,
    t[0],
    t[1],
    t[2],
    1,
  ];
}

export function multiply4(a: Mat4, b: Mat4): Mat4 {
  const out: number[] = new Array<number>(16).fill(0);
  for (let c = 0; c < 4; c += 1) {
    for (let r = 0; r < 4; r += 1) {
      let sum = 0;
      for (let k = 0; k < 4; k += 1) {
        sum += (a[k * 4 + r] ?? 0) * (b[c * 4 + k] ?? 0);
      }
      out[c * 4 + r] = sum;
    }
  }
  return out;
}

export function apply4(m: Mat4, p: Vec3): Vec3 {
  const at = (i: number) => m[i] ?? 0;
  return [
    at(0) * p[0] + at(4) * p[1] + at(8) * p[2] + at(12),
    at(1) * p[0] + at(5) * p[1] + at(9) * p[2] + at(13),
    at(2) * p[0] + at(6) * p[1] + at(10) * p[2] + at(14),
  ];
}

/** A piece's local frame (material corner, Z up from the underside) to assembly space. */
export function pieceMatrix(piece: Pick<PieceRow, 'position' | 'rotation' | 'size'>): Mat4 {
  const r = turnsMatrix(turnsOfRotation(piece.rotation));
  const half: Vec3 = [piece.size[0] / 2, piece.size[1] / 2, piece.size[2] / 2];
  const turned: Vec3 = [
    r[0][0] * half[0] + r[0][1] * half[1] + r[0][2] * half[2],
    r[1][0] * half[0] + r[1][1] * half[1] + r[1][2] * half[2],
    r[2][0] * half[0] + r[2][1] * half[1] + r[2][2] * half[2],
  ];
  return affine(r, [
    piece.position[0] - turned[0],
    piece.position[1] - turned[1],
    piece.position[2] - turned[2],
  ]);
}

const SCENE_OF_JOB: Mat4 = [1, 0, 0, 0, 0, 0, -1, 0, 0, 1, 0, 0, 0, 0, 0, 1];

/** A piece's local frame (part box corner, Z up from the underside) to the scene. */
export function pieceLocalSceneMatrix(
  piece: Pick<PieceRow, 'position' | 'rotation' | 'size'>,
): Mat4 {
  return multiply4(SCENE_OF_JOB, pieceMatrix(piece));
}

/**
 * The matrix that takes a relief mesh as heightfieldMesh builds it (job X,
 * height above the uncut top, minus job Y) to where the piece sits in the
 * scene.
 */
export function pieceSceneMatrix(
  piece: Pick<PieceRow, 'position' | 'rotation' | 'size'>,
  field: Pick<Heightfield, 'cellSize' | 'originX' | 'originY' | 'top' | 'bottom'>,
): Mat4 {
  const left = field.originX - field.cellSize / 2;
  const low = field.originY - field.cellSize / 2;
  const depth = field.top - field.bottom;
  // Mesh (x, h, -y) to local (x - left, y - low, h + depth).
  const meshToLocal: Mat4 = [1, 0, 0, 0, 0, 0, 1, 0, 0, -1, 0, 0, -left, -low, depth, 1];
  return multiply4(SCENE_OF_JOB, multiply4(pieceMatrix(piece), meshToLocal));
}

/** The eight corners of a piece's material box in assembly space. */
export function pieceCorners(piece: Pick<PieceRow, 'position' | 'rotation' | 'size'>): Vec3[] {
  const m = pieceMatrix(piece);
  const out: Vec3[] = [];
  for (let i = 0; i < 8; i += 1) {
    out.push(
      apply4(m, [i & 1 ? piece.size[0] : 0, i & 2 ? piece.size[1] : 0, i & 4 ? piece.size[2] : 0]),
    );
  }
  return out;
}

/** The box round every piece in assembly space, or null with none. */
export function piecesBox(
  pieces: ReadonlyArray<Pick<PieceRow, 'position' | 'rotation' | 'size'>>,
): { min: Vec3; max: Vec3 } | null {
  if (pieces.length === 0) {
    return null;
  }
  const min: [number, number, number] = [Infinity, Infinity, Infinity];
  const max: [number, number, number] = [-Infinity, -Infinity, -Infinity];
  for (const corner of pieces.flatMap(pieceCorners)) {
    for (let k = 0; k < 3; k += 1) {
      min[k] = Math.min(min[k] ?? 0, corner[k] ?? 0);
      max[k] = Math.max(max[k] ?? 0, corner[k] ?? 0);
    }
  }
  return { min, max };
}

/** How long the move arrows are for a piece, mm. */
export function arrowLength(size: Triple): number {
  return Math.max(size[0], size[1], size[2]) * ARROW_SHARE;
}

/** Where the rotation rings sit for a piece. */
export function ringsOf(piece: Pick<PieceRow, 'position' | 'size'>): RingFrame {
  return {
    centre: [piece.position[0], piece.position[1], piece.position[2]],
    radius: arrowLength(piece.size) * RING_SHARE,
  };
}

function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

/**
 * How far along the line through `centre` in direction `axis` (a unit job
 * axis) the point nearest a pointer ray lies, or null when the ray runs
 * along the line.
 */
export function alongAxis(centre: Vec3, axis: Vec3, origin: Vec3, direction: Vec3): number | null {
  const w: Vec3 = [origin[0] - centre[0], origin[1] - centre[1], origin[2] - centre[2]];
  const b = dot(axis, direction);
  const c = dot(direction, direction);
  const d = dot(axis, w);
  const e = dot(direction, w);
  const denominator = c - b * b;
  if (Math.abs(denominator) < 1e-9) {
    return null;
  }
  return (c * d - b * e) / denominator;
}

/** A position moved `distance` along a job axis. */
export function movedAlong(position: Triple, axis: RingAxis, distance: number): Triple {
  const a = axisVector(axis);
  return [
    position[0] + a[0] * distance,
    position[1] + a[1] * distance,
    position[2] + a[2] * distance,
  ];
}

/** A rotation after a further turn of `degrees` about a job axis through the middle. */
export function turnedBy(rotation: Triple, axis: RingAxis, degrees: number): Triple {
  return rotationOfTurns(
    turnedAbout(turnsOfRotation(rotation), axisVector(axis), (degrees * Math.PI) / 180),
  );
}

/** A position moved by a scene space displacement (x, z, -y). */
export function movedByScene(position: Triple, delta: Vec3): Triple {
  return [position[0] + delta[0], position[1] - delta[2], position[2] + delta[1]];
}

/** Tidies a typed or dragged value: a thousandth of a unit, no minus nought. */
export function tidy(value: number): number {
  const rounded = Math.round(value * 1000) / 1000;
  return Object.is(rounded, -0) ? 0 : rounded;
}

/** A piece's local point (part box corner, Z up from the underside) for a point in assembly space. */
export function toPieceLocal(
  piece: Pick<PieceRow, 'position' | 'rotation' | 'size'>,
  point: Vec3,
): Triple {
  const m = pieceMatrix(piece);
  const at = (i: number) => m[i] ?? 0;
  const d: Vec3 = [point[0] - at(12), point[1] - at(13), point[2] - at(14)];
  // The rotation's columns are the local axes in assembly space.
  return [
    at(0) * d[0] + at(1) * d[1] + at(2) * d[2],
    at(4) * d[0] + at(5) * d[1] + at(6) * d[2],
    at(8) * d[0] + at(9) * d[1] + at(10) * d[2],
  ];
}

/** A box in assembly space, min and max corners. */
export interface Box {
  min: Vec3;
  max: Vec3;
}

/** The box round a piece's part in assembly space. */
export function pieceBox(piece: Pick<PieceRow, 'position' | 'rotation' | 'size'>): Box {
  return piecesBox([piece]) ?? { min: [0, 0, 0], max: [0, 0, 0] };
}

/** Distances under which a dragged piece snaps, CSS pixels. */
export const SNAP_PIXELS = 8;
