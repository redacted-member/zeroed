// JSON shapes of assembly.verifyFit: the joints between the placed pieces
// with their clearance per side, measured on the pieces' exact cut edges
// (or the simulation grid where a piece has none), and how far each piece
// moves at a full explode. Lengths are millimetres, assembly space is Z up.

import type { PieceRef, Triple } from './assembly';

/** How a joint, a side of it or one of its faces fits. */
export type FitStatus = 'ok' | 'loose' | 'tight' | 'interference';

/** What a joint was measured on. */
export type FitSource = 'exact' | 'grid';

/** One way a joint's faces face, with the gaps across it. */
export interface FitSide {
  /** The first piece's face normal there, pointing at the second piece. */
  normal: Triple;
  /** Smallest gap, mm; negative where the pieces overlap. */
  min: number;
  max: number;
  /** Area of the faces facing each other, square mm. */
  area: number;
  /** A fit (walls either side), else a resting contact. */
  fit: boolean;
  status: FitStatus;
  /** A round pin in a round hole: the gap all round the axis (`normal`). */
  round?: boolean;
}

/** Two faces facing each other, as triangles in each piece's own frame. */
export interface FitPatch {
  status: FitStatus;
  min: number;
  max: number;
  /** x, y, z per corner, three corners per triangle, on the first piece. */
  a: number[];
  /** The same on the second piece. */
  b: number[];
}

/** A joint between two pieces. */
export interface FitJoint {
  a: PieceRef;
  b: PieceRef;
  status: FitStatus;
  source: FitSource;
  /** The grid's cell size for a grid measurement, mm. */
  cell: number | null;
  sides: FitSide[];
  min: number | null;
  max: number | null;
  /** How far the pieces run into each other, mm. */
  overlapDepth: number;
  /** Material they share, cubic mm. */
  overlapVolume: number;
  /** Box round the joint, min then max corner. */
  bounds: [Triple, Triple];
  patches: FitPatch[];
}

/** A piece's move at a full explode. */
export interface ExplodeOffset {
  piece: PieceRef;
  offset: Triple;
}

export interface VerifyFitReply {
  joints: FitJoint[];
  explode: ExplodeOffset[];
  /** Pieces measured on exact edges; the others on the grid. */
  exact: PieceRef[];
  target: number;
  tolerance: number;
}

/** A job axis a section plane cuts across. */
export type SectionAxis = 'x' | 'y' | 'z';

/** A picked point snapped to its piece's face, assembly space. */
export interface MeasuredPoint {
  point: Triple;
  normal: Triple;
  /** Snapped to the exact cut edges, else the simulation grid. */
  exact: boolean;
}

/** assembly.measure: two points and what lies between them. */
export interface MeasureReply {
  a: MeasuredPoint;
  b: MeasuredPoint;
  distance: number;
  /** b less a along X, Y and Z. */
  delta: Triple;
  parallel: boolean;
  /** Gap between the two faces when they are parallel, else null. */
  faceDistance: number | null;
  /** Angle between the faces' normals, degrees. */
  angle: number;
}

/** assembly.exportAssembly: what was written. */
export interface ExportAssemblyReply {
  path: string;
  pieces: number;
  exact: number;
  triangles: number;
}
