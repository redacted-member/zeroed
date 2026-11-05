// The orientation cube: a unit cube drawn with the 3D camera's rotation, so
// each face shows where that side of the job is. Clicking a face snaps the
// camera to look at it.

import { cameraBasis, type Vec3, type ViewPreset } from '../View3D/camera3d';

export type CubeFace = Exclude<ViewPreset, 'iso'>;

export interface ProjectedFace {
  face: CubeFace;
  /** SVG polygon points, centred on 0, 0 with y down. */
  points: string;
  /** Face centre on screen. */
  centre: { x: number; y: number };
  /** How squarely the face points at the camera, 0 to 1. */
  facing: number;
}

interface FaceDef {
  face: CubeFace;
  normal: Vec3;
  corners: readonly Vec3[];
}

// Scene axes: x right, y up, z towards the front of the job.
const FACES: readonly FaceDef[] = [
  {
    face: 'top',
    normal: [0, 1, 0],
    corners: [
      [-1, 1, 1],
      [1, 1, 1],
      [1, 1, -1],
      [-1, 1, -1],
    ],
  },
  {
    face: 'bottom',
    normal: [0, -1, 0],
    corners: [
      [-1, -1, -1],
      [1, -1, -1],
      [1, -1, 1],
      [-1, -1, 1],
    ],
  },
  {
    face: 'front',
    normal: [0, 0, 1],
    corners: [
      [-1, -1, 1],
      [1, -1, 1],
      [1, 1, 1],
      [-1, 1, 1],
    ],
  },
  {
    face: 'back',
    normal: [0, 0, -1],
    corners: [
      [1, -1, -1],
      [-1, -1, -1],
      [-1, 1, -1],
      [1, 1, -1],
    ],
  },
  {
    face: 'right',
    normal: [1, 0, 0],
    corners: [
      [1, -1, 1],
      [1, -1, -1],
      [1, 1, -1],
      [1, 1, 1],
    ],
  },
  {
    face: 'left',
    normal: [-1, 0, 0],
    corners: [
      [-1, -1, -1],
      [-1, -1, 1],
      [-1, 1, 1],
      [-1, 1, -1],
    ],
  },
];

/** Faces seen at less than this are too thin to click. */
const MIN_FACING = 0.05;

const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

// Adding zero turns -0 into 0.
const round = (value: number) => Math.round(value * 100) / 100 + 0;

/**
 * The faces the camera sees, each projected with half size `half` (screen
 * units per half cube edge), furthest first so nearer faces draw on top.
 */
export function projectCube(theta: number, phi: number, half: number): ProjectedFace[] {
  const { back, right, up } = cameraBasis(theta, phi);
  const toScreen = (v: Vec3) => ({ x: dot(v, right) * half, y: -dot(v, up) * half });
  const out: Array<ProjectedFace & { depth: number }> = [];
  for (const def of FACES) {
    const facing = dot(def.normal, back);
    if (facing < MIN_FACING) {
      continue;
    }
    const screen = def.corners.map(toScreen);
    const centre = toScreen(def.normal);
    out.push({
      face: def.face,
      points: screen.map((p) => `${round(p.x)},${round(p.y)}`).join(' '),
      centre: { x: round(centre.x), y: round(centre.y) },
      facing,
      depth: dot(def.normal, back),
    });
  }
  out.sort((a, b) => a.depth - b.depth);
  return out.map(({ face, points, centre, facing }) => ({ face, points, centre, facing }));
}
